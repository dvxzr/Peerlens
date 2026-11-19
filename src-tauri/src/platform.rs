//! Everything OS-specific lives here, behind cfg gates.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Directories GUI apps on macOS often miss because they don't inherit the shell PATH.
pub fn extra_bin_dirs() -> Vec<PathBuf> {
    let mut v = vec![PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin")];
    #[cfg(target_os = "linux")]
    v.extend([
        PathBuf::from("/home/linuxbrew/.linuxbrew/bin"),
        PathBuf::from("/snap/bin"),
    ]);
    if let Some(h) = dirs::home_dir() {
        v.push(h.join(".local/bin"));
        v.push(h.join(".cargo/bin"));
        v.push(h.join(".npm-global/bin"));
        v.push(h.join(".volta/bin"));
    }
    v
}

/// PATH for child processes: inherited PATH plus the extra dirs, deduplicated.
pub fn child_path() -> String {
    let mut parts: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for d in extra_bin_dirs() {
        if !parts.contains(&d) {
            parts.push(d);
        }
    }
    std::env::join_paths(parts)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Resolve a binary: explicit override → PATH → common dirs → login shell. Cached per name.
pub fn resolve_binary(name: &str, override_path: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = override_path.filter(|p| !p.trim().is_empty()) {
        let p = PathBuf::from(p.trim());
        return is_executable(&p).then_some(p);
    }
    static CACHE: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap().get(name) {
        return hit.clone();
    }
    let found = which::which(name)
        .ok()
        .or_else(|| {
            extra_bin_dirs()
                .into_iter()
                .map(|d| d.join(name))
                .find(|p| is_executable(p))
        })
        .or_else(|| login_shell_which(name));
    tracing::info!(binary = name, path = ?found, "resolved binary");
    cache.lock().unwrap().insert(name.to_string(), found.clone());
    found
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata()
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

#[cfg(unix)]
fn login_shell_which(name: &str) -> Option<PathBuf> {
    // Name is interpolated into a shell command: allow only safe characters.
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let out = std::process::Command::new(shell)
        .args(["-l", "-c", &format!("command -v {name}")])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).lines().last()?.trim());
    is_executable(&p).then_some(p)
}

#[cfg(not(unix))]
fn login_shell_which(_name: &str) -> Option<PathBuf> {
    None
}

/// Put the child in its own process group so Cancel can kill the whole tree.
pub fn new_process_group(cmd: &mut tokio::process::Command) {
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(not(unix))]
    let _ = cmd;
}

/// Kill the process group started by `new_process_group`.
pub fn kill_tree(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::killpg(pid as libc::pid_t, libc::SIGKILL);
    }
    #[cfg(not(unix))]
    let _ = pid; // shortcut: non-unix relies on Child::kill only; add a job object for Windows if ever needed.
}

/// Open an http(s) URL in the default browser.
pub fn open_url(url: &str) -> std::io::Result<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "only http(s) URLs",
        ));
    }
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(not(target_os = "macos"))]
    let opener = "xdg-open";
    std::process::Command::new(opener).arg(url).spawn().map(|_| ())
}

/// A fresh empty directory under the system temp dir; removed on drop.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(prefix: &str) -> std::io::Result<Self> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{seq}", std::process::id()));
        std::fs::create_dir(&p)?; // create_dir (not _all) fails if it already exists
        Ok(Self(p))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Remove `peerlens-*` temp dirs left by a crashed or killed earlier run (another pid).
pub fn cleanup_stale_temp() {
    let me = format!("-{}-", std::process::id());
    let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if (name.starts_with("peerlens-agent-") || name.starts_with("peerlens-repo-")) && !name.contains(&me) {
            let p = e.path();
            let _ = if p.is_dir() {
                std::fs::remove_dir_all(&p)
            } else {
                std::fs::remove_file(&p)
            };
        }
    }
}

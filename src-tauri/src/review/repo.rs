//! Read-only repository snapshot for "repo context": the GitLab archive at the MR head,
//! unpacked into a directory that `cache` keeps between runs.
use crate::error::{AppError, Result};
use crate::gitlab::runner::Glab;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// Default file-name patterns never unpacked (secrets). Editable in Settings.
pub fn default_exclude() -> Vec<String> {
    [
        ".env",
        ".env.*",
        "*.env",
        "*.pem",
        "*.key",
        "*.p12",
        "*.pfx",
        "*.jks",
        "*.keystore",
        "*.kdbx",
        "id_rsa*",
        "id_ed25519*",
        "id_ecdsa*",
        "*.ppk",
        ".npmrc",
        ".pypirc",
        ".netrc",
        ".git-credentials",
        "*.tfvars",
        "*.tfstate*",
        "*secret*",
        "*credential*",
        "*.sqlite",
        "*.db",
    ]
    .map(String::from)
    .to_vec()
}

/// Case-insensitive glob with `*` and `?`, matched against a file name.
pub fn glob_match(pat: &str, name: &str) -> bool {
    fn m(p: &[u8], n: &[u8]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some(b'*'), _) => m(&p[1..], n) || (!n.is_empty() && m(p, &n[1..])),
            (Some(b'?'), Some(_)) => m(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => m(&p[1..], &n[1..]),
            _ => false,
        }
    }
    m(pat.as_bytes(), name.as_bytes())
}

const MAX_FILE: u64 = 1_048_576; // larger files are skipped: not useful context, mostly data/binaries

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SnapshotStats {
    pub files: usize,
    pub bytes: u64,
    pub excluded: Vec<String>,
    pub skipped_large: usize,
    pub skipped_links: usize,
}

/// Unpack a .tar.gz, dropping the archive's top-level folder. Only regular files and directories
/// are written (no symlinks/hardlinks that could point outside), and only plain relative paths.
pub fn extract(tgz: &Path, into: &Path, exclude: &[String], max_total: u64) -> Result<SnapshotStats> {
    let mut st = SnapshotStats::default();
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(std::fs::File::open(tgz)?));
    for entry in ar.entries()? {
        let mut e = entry?;
        let ty = e.header().entry_type();
        if !(ty.is_file() || ty.is_dir()) {
            st.skipped_links += 1;
            continue;
        }
        let rel: PathBuf = e.path()?.components().skip(1).collect();
        if rel.as_os_str().is_empty() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
            continue;
        }
        let dest = into.join(&rel);
        if ty.is_dir() {
            std::fs::create_dir_all(&dest)?;
            continue;
        }
        let name = rel
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if exclude.iter().any(|p| glob_match(p, &name)) {
            st.excluded.push(rel.to_string_lossy().into_owned());
            continue;
        }
        let size = e.size();
        if size > MAX_FILE {
            st.skipped_large += 1;
            continue;
        }
        if st.bytes + size > max_total {
            return Err(AppError::Rejected(format!(
                "unpacked repository is larger than {} MB",
                max_total / 1_048_576
            )));
        }
        if let Some(p) = dest.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut f = std::fs::File::create(&dest)?;
        std::io::copy(&mut e, &mut f)?;
        st.files += 1;
        st.bytes += size;
    }
    Ok(st)
}

/// Download the archive of `project_id` at `sha` through glab (GET) and unpack it into the empty dir `into`.
pub async fn fetch_into(
    g: &Glab,
    project_id: u64,
    sha: &str,
    max_bytes: u64,
    exclude: &[String],
    into: PathBuf,
) -> Result<SnapshotStats> {
    if sha.is_empty() || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::Rejected(format!("sha {sha}")));
    }
    let tgz = into.with_extension("tar.gz");
    let _cleanup = scopeguard_file(&tgz);
    g.download(
        &format!("projects/{project_id}/repository/archive.tar.gz?sha={sha}"),
        &tgz,
        max_bytes,
    )
    .await?;
    let (t, ex) = (tgz.clone(), exclude.to_vec());
    tokio::task::spawn_blocking(move || extract(&t, &into, &ex, max_bytes.saturating_mul(4)))
        .await
        .map_err(|e| AppError::Agent(format!("unpack: {e}")))?
}

/// Deletes the downloaded archive file when dropped.
struct FileGuard(PathBuf);
impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn scopeguard_file(p: &Path) -> FileGuard {
    FileGuard(p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::TempDir;

    #[test]
    fn globbing() {
        assert!(glob_match(".env.*", ".env.local"));
        assert!(glob_match("*.PEM", "server.pem"));
        assert!(glob_match("*secret*", "k8s-Secrets.yaml"));
        assert!(glob_match("id_rsa*", "id_rsa.pub"));
        assert!(!glob_match("*.key", "keys.py"));
        assert!(!glob_match(".env", ".envrc"));
        assert!(glob_match("a?c", "abc") && !glob_match("a?c", "ac"));
    }

    fn tgz(entries: &[(&str, &[u8], tar::EntryType)]) -> TempDir {
        let d = TempDir::new("peerlens-tgz").unwrap();
        let f = std::fs::File::create(d.0.join("a.tar.gz")).unwrap();
        let mut b = tar::Builder::new(flate2::write::GzEncoder::new(f, flate2::Compression::fast()));
        for (path, data, ty) in entries {
            let mut h = tar::Header::new_gnu();
            h.set_entry_type(*ty);
            h.set_size(if *ty == tar::EntryType::Regular {
                data.len() as u64
            } else {
                0
            });
            h.set_mode(0o644);
            if *ty == tar::EntryType::Symlink {
                h.set_link_name(std::str::from_utf8(data).unwrap()).unwrap();
            }
            // set_path rejects "..", so write the raw name bytes to simulate a malicious archive.
            let name = h.as_old_mut().name.as_mut();
            name[..path.len()].copy_from_slice(path.as_bytes());
            h.set_cksum();
            b.append(&h, if *ty == tar::EntryType::Regular { *data } else { &[][..] })
                .unwrap();
        }
        b.into_inner().unwrap().finish().unwrap();
        d
    }

    #[test]
    fn extracts_safely() {
        use tar::EntryType::*;
        let src = tgz(&[
            ("repo-abc/src/main.py", b"print(1)", Regular),
            ("repo-abc/.env", b"TOKEN=x", Regular),
            ("repo-abc/conf/prod.pem", b"---", Regular),
            ("repo-abc/link", b"/etc/passwd", Symlink),
            ("repo-abc/../escape.txt", b"x", Regular),
        ]);
        let out = TempDir::new("peerlens-out").unwrap();
        let st = extract(&src.0.join("a.tar.gz"), &out.0, &default_exclude(), 1 << 20).unwrap();
        assert_eq!(std::fs::read_to_string(out.0.join("src/main.py")).unwrap(), "print(1)");
        assert!(!out.0.join(".env").exists() && !out.0.join("conf/prod.pem").exists());
        assert_eq!(st.excluded, [".env", "conf/prod.pem"]);
        assert!(!out.0.join("link").exists());
        assert_eq!(st.skipped_links, 1);
        assert!(!out.0.parent().unwrap().join("escape.txt").exists() && !out.0.join("escape.txt").exists());
        assert_eq!(st.files, 1);
    }
}

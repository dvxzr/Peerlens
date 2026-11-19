//! The ONLY place that spawns `glab`. Every request is forced to HTTP GET.
use crate::error::{AppError, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone)]
pub struct Glab {
    pub bin: PathBuf,
    /// `--hostname`; None = glab's default host.
    pub host: Option<String>,
}

/// Build the argv for a read-only `glab api` call. Never adds body/field flags,
/// so glab can't switch to POST on its own.
pub fn get_args(endpoint: &str, host: Option<&str>, paginate: bool) -> Result<Vec<String>> {
    let bad = endpoint.is_empty()
        || endpoint.starts_with('-')
        || endpoint.starts_with('/')
        || endpoint.starts_with("graphql")
        || endpoint.contains("..")
        || !endpoint
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/_-.%?&=:,+~".contains(c));
    if bad {
        return Err(AppError::Rejected(endpoint.to_string()));
    }
    let mut args = vec!["api".to_string(), "--method".into(), "GET".into()];
    if let Some(h) = host.filter(|h| !h.is_empty()) {
        if h.starts_with('-') || h.chars().any(char::is_whitespace) {
            return Err(AppError::Rejected(format!("host {h}")));
        }
        args.push("--hostname".into());
        args.push(h.to_string());
    }
    if paginate {
        args.push("--paginate".into());
    }
    args.push(endpoint.to_string());
    Ok(args)
}

impl Glab {
    pub fn from_settings(s: &crate::settings::Settings) -> Result<Self> {
        let bin =
            crate::platform::resolve_binary("glab", Some(&s.glab_path).filter(|p| !p.is_empty()).map(|p| p.as_str()))
                .ok_or_else(|| AppError::BinaryNotFound("glab".into()))?;
        Ok(Self {
            bin,
            host: Some(s.host.clone()).filter(|h| !h.is_empty()),
        })
    }

    fn command(&self, args: &[String]) -> tokio::process::Command {
        tracing::info!(target: "peerlens::glab", argv = ?args, "glab call");
        let mut c = tokio::process::Command::new(&self.bin);
        c.args(args)
            // Neutral cwd so glab doesn't infer a host from some git repo.
            .current_dir(std::env::temp_dir())
            .env("PATH", crate::platform::child_path())
            .env("NO_COLOR", "1")
            .env("GLAB_NO_PROMPT", "1")
            .stdin(Stdio::null())
            .kill_on_drop(true);
        c
    }

    fn fail(&self, endpoint: &str, code: Option<i32>, stderr: &[u8]) -> AppError {
        let stderr = String::from_utf8_lossy(stderr).trim().to_string();
        if stderr.contains("404") {
            return AppError::NotFound(endpoint.to_string());
        }
        AppError::Glab {
            code: code.unwrap_or(-1),
            stderr,
        }
    }

    /// GET a binary body (e.g. a repository archive) into `dest`, aborting above `max_bytes`.
    pub async fn download(&self, endpoint: &str, dest: &Path, max_bytes: u64) -> Result<u64> {
        let args = get_args(endpoint, self.host.as_deref(), false)?;
        let mut child = self
            .command(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut err = child.stderr.take().unwrap();
        let err_task = tokio::spawn(async move {
            let mut b = Vec::new();
            let _ = (&mut err).take(64 * 1024).read_to_end(&mut b).await;
            b
        });
        let mut out = child.stdout.take().unwrap();
        let mut f = tokio::fs::File::create(dest).await?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let n = out.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > max_bytes {
                let _ = child.kill().await;
                return Err(AppError::Rejected(format!(
                    "archive is larger than {} MB (Settings → Repo context)",
                    max_bytes / 1_048_576
                )));
            }
            f.write_all(&buf[..n]).await?;
        }
        f.flush().await?;
        let status = child.wait().await?;
        if !status.success() {
            return Err(self.fail(endpoint, status.code(), &err_task.await.unwrap_or_default()));
        }
        Ok(total)
    }

    /// GET `endpoint` and parse JSON. With `paginate`, glab merges array pages.
    pub async fn get(&self, endpoint: &str, paginate: bool) -> Result<serde_json::Value> {
        let args = get_args(endpoint, self.host.as_deref(), paginate)?;
        let out = self.command(&args).output().await?;
        if !out.status.success() {
            return Err(self.fail(endpoint, out.status.code(), &out.stderr));
        }
        Ok(serde_json::from_slice(&out.stdout)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_get_and_no_body_flags() {
        let a = get_args(
            "projects/1/merge_requests/2/diffs?per_page=100",
            Some("gl.example.com"),
            true,
        )
        .unwrap();
        assert_eq!(&a[..3], ["api", "--method", "GET"]);
        assert_eq!(a.last().unwrap(), "projects/1/merge_requests/2/diffs?per_page=100");
        for f in [
            "-f",
            "-F",
            "--field",
            "--raw-field",
            "--input",
            "--form",
            "-X",
            "POST",
            "PUT",
            "PATCH",
            "DELETE",
        ] {
            assert!(!a.iter().any(|x| x == f), "unexpected {f}");
        }
    }

    #[test]
    fn rejects_flag_injection_and_graphql() {
        for ep in [
            "-X",
            "--method=POST",
            "graphql",
            "user --method POST",
            "",
            "/user",
            "a/../b",
            "x\ny",
        ] {
            assert!(get_args(ep, None, false).is_err(), "accepted {ep:?}");
        }
        assert!(get_args("user", Some("--evil"), false).is_err());
    }

    /// Source-level guard: only this file may spawn glab, and nothing in gitlab/ may
    /// mention write methods or body flags.
    #[test]
    fn no_write_paths_in_source() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(p) = stack.pop() {
            for e in std::fs::read_dir(&p).unwrap() {
                let path = e.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let src = std::fs::read_to_string(&path).unwrap();
                let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                // The test module below lists the forbidden words itself; check code before it.
                let code = src.split("#[cfg(test)]").next().unwrap();
                if rel != "gitlab/runner.rs" {
                    assert!(!code.contains("\"api\""), "{rel} builds a glab api call");
                    assert!(!code.contains("Glab { bin"), "{rel} constructs Glab directly");
                }
                if rel.starts_with("gitlab/") {
                    for w in [
                        "\"POST\"",
                        "\"PUT\"",
                        "\"PATCH\"",
                        "\"DELETE\"",
                        "--field",
                        "--raw-field",
                        "--input",
                        "--form",
                        "\"-f\"",
                        "\"-F\"",
                        "\"-X\"",
                    ] {
                        assert!(!code.contains(w), "{rel} contains {w}");
                    }
                }
            }
        }
    }
}

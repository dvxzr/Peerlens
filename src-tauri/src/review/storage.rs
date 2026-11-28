//! Last review per (project, iid, head_sha) as JSON in the app data dir.
use super::chunking::Skipped;
use crate::agents::parser::Finding;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredReview {
    pub project_id: u64,
    pub iid: u64,
    pub head_sha: String,
    /// Incremental review: the head of the earlier review whose findings were carried over; "" = full review.
    #[serde(default)]
    pub since: String,
    pub agent: String,
    pub model: String,
    #[serde(default)]
    pub effort: String,
    pub language: String,
    pub scope: String,
    pub created_at: u64,
    pub findings: Vec<Finding>,
    pub summaries: Vec<String>,
    /// Agent text shown when nothing parsed as NDJSON.
    pub raw: String,
    pub skipped: Vec<Skipped>,
    pub errors: Vec<String>,
    pub cancelled: bool,
    #[serde(default)]
    pub usage: Option<crate::agents::runner::Usage>,
    /// Tool calls the agent made (repo context), e.g. "Read src/a.py".
    #[serde(default)]
    pub activity: Vec<String>,
    /// Snapshot summary when repo context was used.
    #[serde(default)]
    pub repo: Option<super::repo::SnapshotStats>,
    /// Cache key of that snapshot (review::cache), reused by finding discussions; "" = none.
    #[serde(default)]
    pub snapshot: String,
    /// Discussion threads keyed by finding index.
    #[serde(default)]
    pub threads: std::collections::BTreeMap<usize, Vec<ThreadMsg>>,
}

/// One message in a finding's discussion thread.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ThreadMsg {
    /// "user" or "agent".
    pub role: String,
    pub text: String,
    pub at: u64,
    /// Agent / model that wrote an agent reply, e.g. "Claude / sonnet".
    #[serde(default)]
    pub by: String,
    /// Tool calls the agent made for this reply (repo context), e.g. "Read src/a.py".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub activity: Vec<String>,
}

pub fn default_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("peerlens").join("reviews"))
}

fn sha_ok(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric())
}

pub fn save(dir: &Path, r: &StoredReview) -> Result<()> {
    if !sha_ok(&r.head_sha) {
        return Ok(()); // nothing sensible to key on
    }
    std::fs::create_dir_all(dir)?;
    // Keep only the latest review for this MR.
    let prefix = format!("{}-{}-", r.project_id, r.iid);
    for e in std::fs::read_dir(dir)?.flatten() {
        if e.file_name().to_string_lossy().starts_with(&prefix) {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let p = dir.join(format!("{prefix}{}.json", r.head_sha));
    std::fs::write(p, serde_json::to_vec(r)?)?;
    Ok(())
}

/// Delete every stored review of the MR. Returns how many files were removed.
pub fn delete(dir: &Path, project_id: u64, iid: u64) -> Result<usize> {
    let prefix = format!("{project_id}-{iid}-");
    let mut n = 0;
    for e in std::fs::read_dir(dir)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) && name.ends_with(".json") {
            std::fs::remove_file(e.path())?;
            n += 1;
        }
    }
    Ok(n)
}

/// Whether any stored review still refers to snapshot `key`.
pub fn uses_snapshot(dir: &Path, key: &str) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    rd.flatten()
        .filter_map(|e| serde_json::from_slice::<StoredReview>(&std::fs::read(e.path()).ok()?).ok())
        .any(|r| r.snapshot == key)
}

/// Latest stored review for the MR, whatever head it was made for (caller compares sha → stale).
pub fn load_latest(dir: &Path, project_id: u64, iid: u64) -> Option<StoredReview> {
    let prefix = format!("{project_id}-{iid}-");
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .filter_map(|e| serde_json::from_slice::<StoredReview>(&std::fs::read(e.path()).ok()?).ok())
        .max_by_key(|r| r.created_at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_replaces_and_loads_latest() {
        let d = crate::platform::TempDir::new("peerlens-store").unwrap();
        let mut r = StoredReview {
            project_id: 1,
            iid: 2,
            head_sha: "aaa".into(),
            created_at: 1,
            ..Default::default()
        };
        save(&d.0, &r).unwrap();
        r.head_sha = "bbb".into();
        r.created_at = 2;
        save(&d.0, &r).unwrap();
        assert_eq!(std::fs::read_dir(&d.0).unwrap().count(), 1);
        assert_eq!(load_latest(&d.0, 1, 2).unwrap().head_sha, "bbb");
        assert!(load_latest(&d.0, 1, 3).is_none());
        save(
            &d.0,
            &StoredReview {
                project_id: 1,
                iid: 20,
                head_sha: "ccc".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(delete(&d.0, 1, 2).unwrap(), 1);
        assert!(load_latest(&d.0, 1, 2).is_none());
        assert!(
            load_latest(&d.0, 1, 20).is_some(),
            "other MR untouched (prefix 1-2- vs 1-20-)"
        );
        save(
            &d.0,
            &StoredReview {
                project_id: 1,
                iid: 2,
                head_sha: "bbb".into(),
                created_at: 2,
                ..Default::default()
            },
        )
        .unwrap();
        r.head_sha = "../evil".into();
        save(&d.0, &r).unwrap();
        assert_eq!(load_latest(&d.0, 1, 2).unwrap().head_sha, "bbb");
    }
}

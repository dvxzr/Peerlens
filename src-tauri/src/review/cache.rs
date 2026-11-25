//! Repo snapshots kept between agent runs, so a finding's discussion reads the same code its review read.
//! One immutable entry per (source project, sha): `<key>/tree` (what the agent sees) and `<key>/meta.json`.
//! Entries are evicted by total size (Settings) and age. Only one agent runs at a time (AppState.running),
//! so there are no concurrent downloads; eviction always spares the entry the caller is about to use.
use super::repo::SnapshotStats;
use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::{Path, PathBuf};

const MAX_AGE_SECS: u64 = 14 * 24 * 3600;
const TMP: &str = ".tmp-";

#[derive(Serialize, Deserialize)]
struct Meta {
    stats: SnapshotStats,
    /// Exclude globs the tree was unpacked with; a different list means a refetch.
    exclude: Vec<String>,
    used_at: u64,
}

pub struct Cached {
    pub key: String,
    /// Read-only root the agent gets.
    pub root: PathBuf,
    pub stats: SnapshotStats,
}

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct CacheInfo {
    pub count: usize,
    pub bytes: u64,
}

pub fn default_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("peerlens").join("snapshots"))
}

/// Cache key for a snapshot; None for a sha that is not plain hex (it becomes a path).
pub fn key(project_id: u64, sha: &str) -> Option<String> {
    (!sha.is_empty() && sha.chars().all(|c| c.is_ascii_hexdigit())).then(|| format!("{project_id}-{sha}"))
}

fn key_ok(k: &str) -> bool {
    !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_meta(entry: &Path) -> Option<Meta> {
    serde_json::from_slice(&std::fs::read(entry.join("meta.json")).ok()?).ok()
}

fn write_meta(entry: &Path, m: &Meta) -> Result<()> {
    let tmp = entry.join("meta.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(m)?)?;
    std::fs::rename(tmp, entry.join("meta.json"))?;
    Ok(())
}

/// Removes a half-built entry when the download fails or is cancelled (its future dropped).
struct Partial(Option<PathBuf>);
impl Drop for Partial {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

/// The snapshot `key` from the cache, or built by `fetch(empty_dir)` on a miss (`fetch` runs only then).
/// The entry appears atomically: it is built under a temp name and renamed when complete.
pub async fn acquire<F, Fut>(dir: &Path, key: &str, exclude: &[String], fetch: F) -> Result<Cached>
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = Result<SnapshotStats>>,
{
    if !key_ok(key) {
        return Err(AppError::Rejected(format!("snapshot key {key}")));
    }
    let entry = dir.join(key);
    let hit = read_meta(&entry).filter(|m| m.exclude == exclude && entry.join("tree").is_dir());
    if let Some(mut m) = hit {
        m.used_at = now();
        let _ = write_meta(&entry, &m); // LRU bookkeeping only
        return Ok(Cached {
            key: key.into(),
            root: entry.join("tree"),
            stats: m.stats,
        });
    }
    let tmp = dir.join(format!("{TMP}{key}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("tree"))?;
    let mut partial = Partial(Some(tmp.clone()));
    let stats = fetch(tmp.join("tree")).await?;
    write_meta(
        &tmp,
        &Meta {
            stats: stats.clone(),
            exclude: exclude.to_vec(),
            used_at: now(),
        },
    )?;
    if entry.exists() {
        std::fs::remove_dir_all(&entry)?; // built with another exclude list
    }
    std::fs::rename(&tmp, &entry)?;
    partial.0 = None;
    Ok(Cached {
        key: key.into(),
        root: entry.join("tree"),
        stats,
    })
}

/// Complete entries as (name, meta); unreadable ones are removed, in-progress ones skipped.
fn entries(dir: &Path) -> Vec<(String, Meta)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut v = vec![];
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with(TMP) || !e.path().is_dir() {
            continue;
        }
        match read_meta(&e.path()) {
            Some(m) => v.push((name, m)),
            None => {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    v
}

/// Drop least recently used entries until the total fits `max_bytes`, and entries unused for 14 days.
/// `keep` is never removed. Returns how many entries were removed.
pub fn evict(dir: &Path, max_bytes: u64, keep: &str) -> usize {
    let mut v = entries(dir);
    v.sort_by_key(|(_, m)| std::cmp::Reverse(m.used_at));
    let (now, mut total, mut n) = (now(), 0u64, 0);
    for (name, m) in v {
        if name != keep && (total + m.stats.bytes > max_bytes || now.saturating_sub(m.used_at) > MAX_AGE_SECS) {
            if std::fs::remove_dir_all(dir.join(&name)).is_ok() {
                n += 1;
            }
            continue;
        }
        total += m.stats.bytes;
    }
    n
}

pub fn remove(dir: &Path, key: &str) {
    if key_ok(key) {
        let _ = std::fs::remove_dir_all(dir.join(key));
    }
}

pub fn info(dir: &Path) -> CacheInfo {
    let v = entries(dir);
    CacheInfo {
        count: v.len(),
        bytes: v.iter().map(|(_, m)| m.stats.bytes).sum(),
    }
}

/// Remove every entry (caller makes sure no agent is running).
pub fn clear(dir: &Path) {
    for (name, _) in entries(dir) {
        let _ = std::fs::remove_dir_all(dir.join(name));
    }
}

/// Remove half-built entries left by a crashed or killed earlier run.
pub fn cleanup_partial(dir: &Path) {
    let me = format!("-{}", std::process::id());
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with(TMP) && !name.ends_with(&me) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::TempDir;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn stats(bytes: u64) -> SnapshotStats {
        SnapshotStats {
            files: 1,
            bytes,
            ..Default::default()
        }
    }

    async fn put(dir: &Path, key: &str, bytes: u64, calls: &AtomicUsize) -> Cached {
        acquire(dir, key, &["*.pem".into()], |into| async move {
            calls.fetch_add(1, Ordering::SeqCst);
            std::fs::write(into.join("a.py"), "x")?;
            Ok(stats(bytes))
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn hit_skips_fetch_and_exclude_change_refetches() {
        let d = TempDir::new("peerlens-cache").unwrap();
        let calls = AtomicUsize::new(0);
        let a = put(&d.0, "1-abc", 10, &calls).await;
        let b = put(&d.0, "1-abc", 10, &calls).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1, "second acquire is a cache hit");
        assert_eq!(a.root, b.root);
        assert!(b.root.join("a.py").is_file() && b.root.ends_with("1-abc/tree"));
        // A changed exclude list may hide new secret files: never reuse the old tree.
        acquire(&d.0, "1-abc", &[], |_| async { Ok(stats(10)) }).await.unwrap();
        assert!(!b.root.join("a.py").exists(), "rebuilt from scratch");
        assert_eq!(info(&d.0), CacheInfo { count: 1, bytes: 10 });
    }

    #[tokio::test]
    async fn failed_fetch_leaves_nothing() {
        let d = TempDir::new("peerlens-cache").unwrap();
        let r = acquire(&d.0, "1-abc", &[], |into| async move {
            std::fs::write(into.join("half"), "x")?;
            Err::<SnapshotStats, _>(AppError::Cancelled)
        })
        .await;
        assert!(matches!(r, Err(AppError::Cancelled)));
        assert_eq!(std::fs::read_dir(&d.0).unwrap().count(), 0, "temp entry removed");
        assert!(acquire(&d.0, "../x", &[], |_| async { Ok(stats(1)) }).await.is_err());
        assert_eq!(key(1, "../x"), None);
    }

    #[tokio::test]
    async fn evicts_least_recently_used_but_spares_keep() {
        let d = TempDir::new("peerlens-cache").unwrap();
        let calls = AtomicUsize::new(0);
        for (k, used) in [("1-a", 100), ("1-b", 200), ("1-c", 300)] {
            put(&d.0, k, 40, &calls).await;
            let e = d.0.join(k);
            let mut m = read_meta(&e).unwrap();
            m.used_at = now() - 1000 + used;
            write_meta(&e, &m).unwrap();
        }
        // 100 bytes fit two 40-byte entries: the oldest one ("1-a") goes...
        assert_eq!(evict(&d.0, 100, "1-c"), 1);
        assert!(!d.0.join("1-a").exists() && d.0.join("1-b").exists());
        // ...but the entry in use survives even when it is the oldest and over budget.
        assert_eq!(evict(&d.0, 0, "1-b"), 1);
        assert!(d.0.join("1-b").exists() && !d.0.join("1-c").exists());
        // Unused for longer than MAX_AGE: removed regardless of size.
        let mut m = read_meta(&d.0.join("1-b")).unwrap();
        m.used_at = now() - MAX_AGE_SECS - 10;
        write_meta(&d.0.join("1-b"), &m).unwrap();
        assert_eq!(evict(&d.0, u64::MAX, ""), 1);
        assert_eq!(info(&d.0).count, 0);
    }

    #[test]
    fn cleanup_and_clear() {
        let d = TempDir::new("peerlens-cache").unwrap();
        std::fs::create_dir_all(d.0.join(".tmp-1-a-999999")).unwrap();
        std::fs::create_dir_all(d.0.join("1-broken")).unwrap(); // no meta.json
        cleanup_partial(&d.0);
        assert!(!d.0.join(".tmp-1-a-999999").exists());
        assert_eq!(info(&d.0).count, 0, "entry without meta is dropped");
        assert!(!d.0.join("1-broken").exists());
        let e = d.0.join("1-ok");
        std::fs::create_dir_all(e.join("tree")).unwrap();
        write_meta(
            &e,
            &Meta {
                stats: stats(5),
                exclude: vec![],
                used_at: now(),
            },
        )
        .unwrap();
        remove(&d.0, "../1-ok");
        assert_eq!(info(&d.0).count, 1, "invalid key ignored");
        clear(&d.0);
        assert_eq!(info(&d.0).count, 0);
    }
}

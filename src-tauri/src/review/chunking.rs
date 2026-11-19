use crate::diff::FileDiff;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct Skipped {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct Plan {
    /// Each chunk is a list of indices into the files slice; one agent run per chunk.
    pub chunks: Vec<Vec<usize>>,
    pub skipped: Vec<Skipped>,
}

const LOCK_FILES: [&str; 12] = [
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "poetry.lock",
    "Pipfile.lock",
    "Gemfile.lock",
    "composer.lock",
    "go.sum",
    "uv.lock",
    "bun.lockb",
    "flake.lock",
];

pub fn skip_reason(f: &FileDiff) -> Option<&'static str> {
    let p = if f.new_path.is_empty() {
        &f.old_path
    } else {
        &f.new_path
    };
    let name = p.rsplit('/').next().unwrap_or(p);
    if f.too_large || f.collapsed {
        Some("too large (no diff from GitLab)")
    } else if f.binary || f.lines.is_empty() {
        Some("binary or empty")
    } else if LOCK_FILES.contains(&name) || name.ends_with(".lock") {
        Some("lock file")
    } else if f.generated
        || name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || name.ends_with(".map")
        || name.ends_with(".snap")
    {
        Some("generated")
    } else {
        None
    }
}

/// Size of the text we'd send for a file (prompt rendering adds ~12 bytes per line).
pub fn file_bytes(f: &FileDiff) -> usize {
    f.lines.iter().map(|l| l.t.len() + 14).sum::<usize>() + f.new_path.len() + 32
}

/// Whole MR in one chunk if it fits `max_bytes`, else one chunk per file.
/// `only` restricts the review to one file path (scope "current file").
pub fn plan(files: &[FileDiff], only: Option<&str>, max_bytes: usize) -> Plan {
    let mut p = Plan::default();
    let mut keep = Vec::new();
    for (i, f) in files.iter().enumerate() {
        if only.is_some_and(|o| o != f.new_path && o != f.old_path) {
            continue;
        }
        match skip_reason(f) {
            Some(r) => p.skipped.push(Skipped {
                path: f.new_path.clone(),
                reason: r.into(),
            }),
            None => keep.push(i),
        }
    }
    let total: usize = keep.iter().map(|&i| file_bytes(&files[i])).sum();
    // shortcut: a single file larger than max_bytes is still sent whole; split by hunks if that becomes a problem.
    if keep.is_empty() {
    } else if total <= max_bytes {
        p.chunks.push(keep);
    } else {
        p.chunks = keep.into_iter().map(|i| vec![i]).collect();
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::from_raw;
    use crate::gitlab::models::RawDiff;

    fn f(path: &str, diff: &str) -> FileDiff {
        from_raw(RawDiff {
            new_path: path.into(),
            old_path: path.into(),
            diff: diff.into(),
            ..Default::default()
        })
    }

    #[test]
    fn skips_lock_binary_generated() {
        let mut g = f("gen.rs", "@@ -1 +1 @@\n+a\n");
        g.generated = true;
        let files = vec![
            f("a.rs", "@@ -1 +1 @@\n+a\n"),
            f("Cargo.lock", "@@ -1 +1 @@\n+a\n"),
            f("x.png", ""),
            g,
            f("b.min.js", "@@ -1 +1 @@\n+a\n"),
        ];
        let p = plan(&files, None, 1_000_000);
        assert_eq!(p.chunks, vec![vec![0]]);
        let reasons: Vec<_> = p.skipped.iter().map(|s| s.reason.as_str()).collect();
        assert_eq!(reasons, ["lock file", "binary or empty", "generated", "generated"]);
    }

    #[test]
    fn splits_per_file_when_too_big() {
        let big = format!("@@ -1,0 +1,100 @@\n{}", "+xxxxxxxxxxxxxxxxxxxx\n".repeat(100));
        let files = vec![f("a", &big), f("b", &big), f("c", "@@ -1 +1 @@\n+a\n")];
        assert_eq!(plan(&files, None, 1_000_000).chunks, vec![vec![0, 1, 2]]);
        assert_eq!(plan(&files, None, 3000).chunks, vec![vec![0], vec![1], vec![2]]);
        assert_eq!(plan(&files, Some("b"), 3000).chunks, vec![vec![1]]);
        assert!(plan(&files, Some("nope"), 3000).chunks.is_empty());
    }
}

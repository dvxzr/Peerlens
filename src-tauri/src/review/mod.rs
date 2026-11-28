pub mod cache;
pub mod chunking;
pub mod prompt;
pub mod repo;
pub mod storage;

use crate::agents::parser::{parse_line, Item, LineAssembler};
use crate::agents::runner::{self, AgentEvent, RunSpec, Usage};
use crate::diff::FileDiff;
use crate::error::AppError;
use crate::gitlab::models::MrDetails;
use serde::Serialize;
use storage::StoredReview;
use tokio::sync::watch;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReviewEvent {
    Started {
        chunks: usize,
        skipped: Vec<chunking::Skipped>,
    },
    Progress {
        chunk: usize,
        total: usize,
        files: Vec<String>,
    },
    Finding {
        finding: crate::agents::parser::Finding,
    },
    Summary {
        text: String,
    },
    Usage {
        usage: Usage,
    },
    /// Repo context: "downloading" | "ready".
    Repo {
        stage: String,
        stats: Option<repo::SnapshotStats>,
    },
    Activity {
        text: String,
    },
    Error {
        message: String,
    },
    Done {
        review: Box<StoredReview>,
    },
}

/// Events of one reply in a finding's discussion thread.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThreadEvent {
    Text {
        text: String,
    },
    Usage {
        usage: Usage,
    },
    /// Repo context: "downloading" (cache miss) | "ready".
    Repo {
        stage: String,
    },
    /// A tool call, formatted like ReviewEvent::Activity.
    Activity {
        text: String,
    },
    /// Something the user should know that does not stop the reply.
    Notice {
        text: String,
    },
    Done {
        thread: Vec<storage::ThreadMsg>,
    },
}

/// "Read src/a.py": a tool call with paths shown relative to the snapshot root.
pub fn tool_text(repo: Option<&std::path::Path>, name: &str, detail: &str) -> String {
    let root = repo.map(|r| r.to_string_lossy().into_owned()).unwrap_or_default();
    if root.is_empty() {
        return format!("{name} {detail}");
    }
    format!("{name} {}", detail.replace(&format!("{root}/"), "").replace(&root, "."))
}

/// Mark a finding anchored if its lines exist on the new side of that file; fill the snippet from the diff.
pub fn anchor(f: &mut crate::agents::parser::Finding, files: &[FileDiff]) {
    let Some(fd) = files.iter().find(|d| d.new_path == f.file || d.old_path == f.file) else {
        return;
    };
    let Some(s) = f.start_line else { return };
    let e = f.end_line.unwrap_or(s);
    f.anchored = fd.has_new_line(s) && fd.has_new_line(e);
    if f.anchored && f.snippet.as_deref().unwrap_or("").trim().is_empty() {
        let lines: Vec<&str> = fd
            .lines
            .iter()
            .filter(|l| l.n.is_some_and(|n| n >= s && n <= e))
            .map(|l| l.t.as_str())
            .take(40)
            .collect();
        f.snippet = Some(lines.join("\n"));
    }
}

/// Whether the user has not closed this finding (open, or open but outdated).
pub fn is_open(f: &crate::agents::parser::Finding) -> bool {
    f.status.is_empty() || f.status == "outdated"
}

/// First new-side line where the finding's snippet now starts in `files` (closest to its old line), if any.
fn relocate(f: &crate::agents::parser::Finding, files: &[FileDiff]) -> Option<u32> {
    let want: Vec<&str> = f
        .snippet
        .as_deref()?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let fd = files.iter().find(|d| d.new_path == f.file)?;
    let have: Vec<(u32, &str)> = fd
        .lines
        .iter()
        .filter(|l| matches!(l.k, '+' | ' '))
        .filter_map(|l| Some((l.n?, l.t.trim())))
        .filter(|(_, t)| !t.is_empty())
        .collect();
    if want.is_empty() || have.len() < want.len() {
        return None;
    }
    let old = f.start_line?;
    have.windows(want.len())
        .filter(|w| w.iter().zip(&want).all(|((_, a), b)| a == b))
        .map(|w| w[0].0)
        .min_by_key(|n| n.abs_diff(old))
}

/// Earlier findings after the MR moved to a new head. In files the new commits (`changed`) did not touch,
/// lines stay valid. In touched files a finding follows its snippet, or becomes "outdated" when it is gone.
/// Closed findings keep their status. All are re-anchored to the current MR diff (`files`).
pub fn carry_over(findings: &mut [crate::agents::parser::Finding], changed: &[FileDiff], files: &[FileDiff]) {
    for f in findings {
        let touched = changed.iter().any(|c| c.new_path == f.file || c.old_path == f.file);
        if let (true, Some(s)) = (touched, f.start_line) {
            match relocate(f, files) {
                Some(n) => {
                    f.end_line = f
                        .end_line
                        .map(|e| (e as i64 + n as i64 - s as i64).max(n as i64) as u32);
                    f.start_line = Some(n);
                }
                None if f.status.is_empty() => f.status = "outdated".into(),
                None => {}
            }
        }
        f.anchored = false;
        anchor(f, files);
    }
}

/// Usage guards so reviews stay human-paced.
#[derive(Debug, Clone)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_chunks: usize,
    pub chunk_delay: std::time::Duration,
    pub max_usage_pct: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 1_000_000,
            max_chunks: 100,
            chunk_delay: std::time::Duration::ZERO,
            max_usage_pct: 100.0,
        }
    }
}

/// Why the review must stop because of subscription usage, if it must.
pub fn usage_stop(u: &Usage, max_pct: f64) -> Option<String> {
    if u.blocked() {
        return Some("Claude reports the usage limit is reached. Remaining parts were not sent.".into());
    }
    let pct = u.max_pct();
    (pct >= max_pct).then(|| {
        format!("Claude usage is at {pct:.0}% (your pause threshold is {max_pct:.0}%). Remaining parts were not sent.")
    })
}

/// Run the whole review: plan chunks, run the agent per chunk sequentially, stream events.
#[allow(clippy::too_many_arguments)]
pub async fn run_review(
    spec: &RunSpec,
    mr: &MrDetails,
    files: &[FileDiff],
    only_file: Option<&str>,
    limits: &Limits,
    mut base: StoredReview,
    incremental: Option<&str>,
    cancel: &mut watch::Receiver<bool>,
    emit: &(dyn Fn(ReviewEvent) + Sync),
) -> StoredReview {
    let plan = chunking::plan(files, only_file, limits.max_bytes);
    base.skipped = plan.skipped.clone();
    let total = plan.chunks.len();
    if total > limits.max_chunks {
        let m = format!("This review needs {total} agent runs; the limit is {}. Review a single file or raise the limit in Settings.", limits.max_chunks);
        emit(ReviewEvent::Error { message: m.clone() });
        base.errors.push(m);
        return base;
    }
    emit(ReviewEvent::Started {
        chunks: total,
        skipped: plan.skipped,
    });
    let mut raw_all = String::new();
    for (ci, chunk) in plan.chunks.iter().enumerate() {
        if *cancel.borrow() {
            base.cancelled = true;
            break;
        }
        if let Some(m) = base.usage.as_ref().and_then(|u| usage_stop(u, limits.max_usage_pct)) {
            emit(ReviewEvent::Error { message: m.clone() });
            base.errors.push(m);
            break;
        }
        if ci > 0 && !limits.chunk_delay.is_zero() {
            tokio::select! {
                _ = tokio::time::sleep(limits.chunk_delay) => {}
                _ = cancel.changed() => { base.cancelled = true; break; }
            }
        }
        let chunk_files: Vec<&FileDiff> = chunk.iter().map(|&i| &files[i]).collect();
        emit(ReviewEvent::Progress {
            chunk: ci + 1,
            total,
            files: chunk_files.iter().map(|f| f.new_path.clone()).collect(),
        });
        let prompt = prompt::user_prompt(
            mr,
            &chunk_files,
            (total > 1).then_some((ci + 1, total)),
            spec.repo.as_deref(),
            incremental,
        );
        let mut asm = LineAssembler::default();
        let mut raw = String::new();
        let mut parsed_any = false;
        let handle = |line: &str, base: &mut StoredReview, parsed_any: &mut bool| match parse_line(line) {
            Some(Item::Finding(mut f)) => {
                *parsed_any = true;
                anchor(&mut f, files);
                emit(ReviewEvent::Finding { finding: f.clone() });
                base.findings.push(f);
            }
            Some(Item::Summary(t)) => {
                *parsed_any = true;
                emit(ReviewEvent::Summary { text: t.clone() });
                base.summaries.push(t);
            }
            None => {}
        };
        let res = runner::run(spec, &prompt, cancel, |e| match e {
            AgentEvent::Text(t) => {
                raw.push_str(&t);
                for l in asm.push(&t) {
                    handle(&l, &mut base, &mut parsed_any);
                }
            }
            AgentEvent::Usage(u) => {
                emit(ReviewEvent::Usage { usage: u.clone() });
                base.usage = Some(u);
            }
            AgentEvent::Error(m) => {
                emit(ReviewEvent::Error { message: m.clone() });
                base.errors.push(m);
            }
            AgentEvent::Tool(name, detail) => {
                let text = tool_text(spec.repo.as_deref(), &name, &detail);
                emit(ReviewEvent::Activity { text: text.clone() });
                if base.activity.len() < 500 {
                    base.activity.push(text);
                }
            }
        })
        .await;
        if let Some(l) = asm.finish() {
            handle(&l, &mut base, &mut parsed_any);
        }
        if !parsed_any && !raw.trim().is_empty() {
            raw_all.push_str(&raw);
            raw_all.push('\n');
        }
        match res {
            Ok(out) if !out.stderr.trim().is_empty() => tracing::warn!(stderr = %out.stderr.trim(), "agent stderr"),
            Ok(_) => {}
            Err(AppError::Cancelled) => {
                base.cancelled = true;
                break;
            }
            Err(e) => {
                let m = format!("chunk {}/{total}: {e}", ci + 1);
                emit(ReviewEvent::Error { message: m.clone() });
                base.errors.push(m);
            }
        }
    }
    base.raw = raw_all;
    base
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::parser::Finding;
    use crate::diff::from_raw;
    use crate::gitlab::models::RawDiff;

    fn fd(path: &str, diff: &str) -> FileDiff {
        from_raw(RawDiff {
            old_path: path.into(),
            new_path: path.into(),
            diff: diff.into(),
            ..Default::default()
        })
    }
    fn finding(file: &str, line: u32, snippet: &str) -> Finding {
        Finding {
            file: file.into(),
            start_line: Some(line),
            end_line: Some(line + 1),
            snippet: Some(snippet.into()),
            ..Default::default()
        }
    }

    #[test]
    fn carry_over_follows_moved_code_and_marks_gone_code_outdated() {
        // Current MR diff: two lines were inserted above `call(x)`, which moved from line 2 to line 4.
        let files = vec![
            fd(
                "a.rs",
                "@@ -1,3 +1,5 @@\n fn f() {\n+    let a = 1;\n+    let b = 2;\n     call(x);\n+    more();\n }\n",
            ),
            fd("b.rs", "@@ -1,2 +1,2 @@\n-old()\n+new()\n keep()\n"),
        ];
        let changed = files.clone(); // the new commits touched both files
        let mut fs = vec![
            finding("a.rs", 2, "    call(x);\n    more();"),
            finding("b.rs", 1, "old()"),
            Finding {
                status: "dismissed".into(),
                ..finding("b.rs", 1, "gone()")
            },
            finding("c.rs", 7, "untouched"),
        ];
        carry_over(&mut fs, &changed, &files);
        assert_eq!(
            (fs[0].start_line, fs[0].end_line, fs[0].status.as_str()),
            (Some(4), Some(5), "")
        );
        assert!(fs[0].anchored);
        assert_eq!(fs[1].status, "outdated", "snippet no longer in the diff");
        assert_eq!(fs[2].status, "dismissed", "closed findings keep their status");
        assert_eq!(
            (fs[3].start_line, fs[3].status.as_str()),
            (Some(7), ""),
            "untouched file keeps its lines"
        );
        assert!(!fs[3].anchored, "not in the MR diff any more");
    }

    #[test]
    fn relocate_picks_the_occurrence_closest_to_the_old_line() {
        let files = vec![fd("a.rs", "@@ -1,0 +1,5 @@\n+x()\n+y\n+z\n+w\n+x()\n")];
        assert_eq!(relocate(&finding("a.rs", 5, "x()"), &files), Some(5));
        assert_eq!(relocate(&finding("a.rs", 2, "  x()  "), &files), Some(1));
        assert_eq!(relocate(&finding("a.rs", 2, "\n \n"), &files), None, "blank snippet");
    }
}

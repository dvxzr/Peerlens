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

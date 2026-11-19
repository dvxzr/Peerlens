//! Full review flow with fake glab + fake agent. No network, no real agent.
use peerlens_lib::agents::{runner::RunSpec, AgentKind};
use peerlens_lib::gitlab::{api, runner::Glab};
use peerlens_lib::review::{run_review, storage::StoredReview, Limits, ReviewEvent};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const FIX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures");

fn log_base() -> String {
    std::env::temp_dir()
        .join(format!("fake-agent-{}", std::process::id()))
        .to_string_lossy()
        .into_owned()
}

fn setup() {
    // Same values in every test, so parallel tests don't race on them.
    std::env::set_var("GITLAB_TOKEN", "secret");
    std::env::set_var("GIT_ASKPASS", "/bin/false");
    std::env::set_var("FAKE_AGENT_LOG", log_base());
}

async fn review(
    mode: &str,
    timeout: Duration,
    cancel_after: Option<Duration>,
) -> (StoredReview, Vec<ReviewEvent>, Duration) {
    review_with(mode, timeout, cancel_after, Limits::default()).await
}

async fn review_with(
    mode: &str,
    timeout: Duration,
    cancel_after: Option<Duration>,
    limits: Limits,
) -> (StoredReview, Vec<ReviewEvent>, Duration) {
    setup();
    let g = Glab {
        bin: format!("{FIX}/fake-glab").into(),
        host: None,
    };
    let mr = api::mr_details(&g, 10, 1).await.unwrap();
    let files = api::mr_diffs(&g, 10, 1).await.unwrap();
    let spec = RunSpec {
        kind: AgentKind::Claude,
        bin: format!("{FIX}/fake-agent").into(),
        model: Some(mode.into()),
        effort: None,
        system_prompt: "sys".into(),
        timeout,
        repo: None,
        known_features: None,
    };
    let (tx, mut rx) = tokio::sync::watch::channel(false);
    // cancel_after = Some(d): cancel d after the first finding arrives (deterministic under load).
    let events = Mutex::new(Vec::new());
    let emit = |e: ReviewEvent| {
        if let (Some(d), ReviewEvent::Finding { .. }) = (cancel_after, &e) {
            let tx = tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(d).await;
                let _ = tx.send(true);
            });
        }
        events.lock().unwrap().push(e);
    };
    let t = Instant::now();
    let r = run_review(
        &spec,
        &mr,
        &files,
        None,
        &limits,
        StoredReview::default(),
        &mut rx,
        &emit,
    )
    .await;
    (r, events.into_inner().unwrap(), t.elapsed())
}

#[tokio::test]
async fn streams_findings_and_anchors_them() {
    let (r, ev, _) = review("ok", Duration::from_secs(20), None).await;
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(r.findings.len(), 2);
    let a = &r.findings[0];
    assert!(a.anchored && a.title == "Mutable default argument");
    assert!(
        a.snippet.as_deref().unwrap().contains("attempts"),
        "snippet filled from diff"
    );
    assert!(!r.findings[1].anchored, "line 999 is not in the diff");
    assert_eq!(r.summaries, ["One real bug."]);
    assert!(r.raw.is_empty());
    // poetry.lock (generated) and logo.png (binary) are skipped and listed.
    assert_eq!(r.skipped.len(), 2);
    assert!(matches!(ev[0], ReviewEvent::Started { chunks: 1, .. }));
    assert!(ev.iter().any(|e| matches!(e, ReviewEvent::Finding { .. })));

    let log = std::fs::read_to_string(format!("{}.ok", log_base())).unwrap();
    assert!(
        !log.contains("GITLAB_TOKEN") && !log.contains("GIT_ASKPASS"),
        "credentials leaked:\n{log}"
    );
    assert!(log.contains("files: 0"), "cwd must be an empty dir:\n{log}");
    assert!(log.contains("--tools  --disallowedTools"), "tools disabled:\n{log}");
    let pid = log.lines().find_map(|l| l.strip_prefix("pid: ")).unwrap();
    let pgid = log.lines().find_map(|l| l.strip_prefix("pgid: ")).unwrap();
    assert_eq!(pid, pgid, "agent runs in its own process group");
}

#[tokio::test]
async fn falls_back_to_raw_text() {
    let (r, _, _) = review("garbage", Duration::from_secs(20), None).await;
    assert!(r.findings.is_empty());
    assert!(r.raw.contains("prose instead"), "{:?}", r.raw);
}

#[tokio::test]
async fn surfaces_nonzero_exit_and_stderr() {
    let (r, ev, _) = review("fail", Duration::from_secs(20), None).await;
    assert!(r.errors[0].contains("boom: not logged in"), "{:?}", r.errors);
    assert!(ev.iter().any(|e| matches!(e, ReviewEvent::Error { .. })));
}

#[tokio::test]
async fn cancel_kills_process_tree_quickly() {
    let (r, _, took) = review("slow", Duration::from_secs(60), Some(Duration::from_millis(300))).await;
    assert!(r.cancelled);
    assert!(took < Duration::from_secs(5), "took {took:?}");
    assert_eq!(r.findings.len(), 1, "finding before cancel is kept");
    let log = std::fs::read_to_string(format!("{}.slow", log_base())).unwrap();
    let pid = log.lines().find_map(|l| l.strip_prefix("pid: ")).unwrap();
    // The shell's child `sleep 30` is in the same group; the whole group must be gone
    // (allow a moment for orphaned zombies to be reaped).
    let gone = (0..20).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        let out = std::process::Command::new("pgrep").args(["-g", pid]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().is_empty()
    });
    assert!(gone, "group {pid} still alive");
}

#[tokio::test]
async fn times_out() {
    let (r, _, took) = review("slow", Duration::from_secs(1), None).await;
    assert!(took < Duration::from_secs(5));
    assert!(r.errors.iter().any(|e| e.contains("timed out")), "{:?}", r.errors);
}

#[tokio::test]
async fn pauses_when_usage_is_over_threshold() {
    // Tiny max_bytes → one agent run per file (2 reviewable files); usage 90% ≥ 80% → second run never starts.
    let limits = Limits {
        max_bytes: 10,
        max_usage_pct: 80.0,
        ..Limits::default()
    };
    let (r, ev, _) = review_with("hot", Duration::from_secs(20), None, limits).await;
    assert!(matches!(ev[0], ReviewEvent::Started { chunks: 2, .. }), "{:?}", ev[0]);
    assert!(r.errors.iter().any(|e| e.contains("90%")), "{:?}", r.errors);
    assert!(ev.iter().any(|e| matches!(e, ReviewEvent::Usage { .. })));
    assert!((r.usage.as_ref().unwrap().max_pct() - 90.0).abs() < 1e-6);
    let progress = ev.iter().filter(|e| matches!(e, ReviewEvent::Progress { .. })).count();
    assert_eq!(progress, 1, "only the first part may be sent");
}

#[tokio::test]
async fn refuses_too_many_agent_runs() {
    let limits = Limits {
        max_bytes: 10,
        max_chunks: 0,
        ..Limits::default()
    };
    let (r, ev, _) = review_with("ok", Duration::from_secs(20), None, limits).await;
    assert!(r.errors[0].contains("agent runs"), "{:?}", r.errors);
    assert!(
        !ev.iter().any(|e| matches!(e, ReviewEvent::Progress { .. })),
        "nothing may be sent"
    );
}

#[tokio::test]
async fn repo_context_snapshot_is_readonly_input_and_deleted() {
    use peerlens_lib::review::repo;
    setup();
    let g = Glab {
        bin: format!("{FIX}/fake-glab").into(),
        host: None,
    };
    let mr = api::mr_details(&g, 10, 1).await.unwrap();
    let files = api::mr_diffs(&g, 10, 1).await.unwrap();
    let snap = repo::snapshot(&g, 10, "abc123", 10 << 20, &repo::default_exclude())
        .await
        .unwrap();
    let root = snap.root().to_path_buf();
    assert_eq!(snap.stats.files, 2);
    assert_eq!(snap.stats.excluded, [".env"]);
    assert_eq!(snap.stats.skipped_links, 1);
    let spec = RunSpec {
        kind: AgentKind::Claude,
        bin: format!("{FIX}/fake-agent").into(),
        model: Some("repo".into()),
        effort: None,
        system_prompt: "sys".into(),
        timeout: Duration::from_secs(20),
        repo: Some(root.clone()),
        known_features: None,
    };
    let (_tx, mut rx) = tokio::sync::watch::channel(false);
    let events = Mutex::new(Vec::new());
    let emit = |e: ReviewEvent| events.lock().unwrap().push(e);
    let r = run_review(
        &spec,
        &mr,
        &files,
        None,
        &Limits::default(),
        StoredReview::default(),
        &mut rx,
        &emit,
    )
    .await;
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(r.activity, ["Read src/auth.py"]);
    let log = std::fs::read_to_string(format!("{}.repo", log_base())).unwrap();
    assert!(log.contains("--tools Read,Grep,Glob"), "{log}");
    assert!(log.contains("repo-files: src/auth.py tests/test_auth.py"), "{log}");
    drop(snap);
    assert!(!root.exists(), "snapshot must be deleted after the review");
}

async fn codex_review(mode: &str, repo: Option<std::path::PathBuf>) -> StoredReview {
    setup();
    let g = Glab {
        bin: format!("{FIX}/fake-glab").into(),
        host: None,
    };
    let mr = api::mr_details(&g, 10, 1).await.unwrap();
    let files = api::mr_diffs(&g, 10, 1).await.unwrap();
    let spec = RunSpec {
        kind: AgentKind::Codex,
        bin: format!("{FIX}/fake-codex").into(),
        model: Some(mode.into()),
        effort: Some("high".into()),
        system_prompt: "SYS".into(),
        timeout: Duration::from_secs(20),
        repo,
        known_features: None,
    };
    let (_tx, mut rx) = tokio::sync::watch::channel(false);
    run_review(
        &spec,
        &mr,
        &files,
        None,
        &Limits::default(),
        StoredReview::default(),
        &mut rx,
        &|_| {},
    )
    .await
}

#[tokio::test]
async fn codex_review_parses_agent_messages() {
    let r = codex_review("ok", None).await;
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(r.findings.len(), 1);
    assert!(r.findings[0].anchored);
    assert_eq!(r.summaries, ["Needs changes."]);
    let log = std::fs::read_to_string(format!("{}.codex-ok", log_base())).unwrap();
    assert!(
        log.contains("--sandbox read-only")
            && log.contains("--disable shell_tool")
            && log.contains("--ignore-user-config"),
        "{log}"
    );
    assert!(!log.contains("GITLAB_TOKEN"), "credentials leaked");
    let home = log.lines().find_map(|l| l.strip_prefix("home: ")).unwrap();
    assert!(
        home.contains("peerlens-agent-"),
        "HOME must be the empty temp dir, got {home}"
    );
}

#[tokio::test]
async fn codex_not_logged_in_shows_its_own_error() {
    let r = codex_review("nologin", None).await;
    assert!(
        r.errors.iter().any(|e| e.contains("401 Unauthorized")),
        "{:?}",
        r.errors
    );
    assert!(
        !r.errors.iter().any(|e| e.contains("noisy log line")),
        "stderr noise must not be the message: {:?}",
        r.errors
    );
}

#[tokio::test]
async fn codex_repo_context_reports_commands() {
    let d = peerlens_lib::platform::TempDir::new("peerlens-fake-repo").unwrap();
    let r = codex_review("repo", Some(d.0.clone())).await;
    assert_eq!(
        r.activity,
        ["shell rg -n check src"],
        "path shown relative to the snapshot"
    );
    let log = std::fs::read_to_string(format!("{}.codex-repo", log_base())).unwrap();
    assert!(
        log.contains(&format!("--cd {}", d.0.display())) && !log.contains("--disable shell_tool"),
        "{log}"
    );
}

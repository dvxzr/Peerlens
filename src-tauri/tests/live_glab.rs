//! Hits the real GitLab through your glab login. Read-only. Run with:
//! cargo test --test live_glab -- --ignored --nocapture
use peerlens_lib::gitlab::{api, runner::Glab};

#[tokio::test]
#[ignore]
async fn live_read_flow() {
    let g = Glab::from_settings(&peerlens_lib::settings::load()).expect("glab");
    let me = api::current_user(&g).await.unwrap();
    let mrs = api::my_mrs(&g, &me).await.unwrap();
    println!("{} open MRs for {}", mrs.len(), me.username);
    let Some(m) = mrs.first() else { return };
    let d = api::mr_details(&g, m.project_id, m.iid).await.unwrap();
    let c = api::mr_commits(&g, m.project_id, m.iid).await.unwrap();
    let f = api::mr_diffs(&g, m.project_id, m.iid).await.unwrap();
    let ds = api::discussions(&g, m.project_id, m.iid).await.unwrap();
    println!(
        "{} !{}: {} commits, {} files, {} discussions, pipeline {:?}",
        d.references.full,
        d.iid,
        c.len(),
        f.len(),
        ds.len(),
        d.head_pipeline.map(|p| p.status)
    );
    let cd = api::commit_diff(&g, m.project_id, &c[0].id).await.unwrap();
    println!("first commit touches {} files", cd.len());
}

/// Real Claude review of your smallest open MR (costs a few cents). Run with:
/// cargo test --test live_glab live_claude_review -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn live_claude_review() {
    use peerlens_lib::agents::{runner::RunSpec, AgentKind};
    use peerlens_lib::review::{prompt, run_review, storage::StoredReview, Limits, ReviewEvent};
    let s = peerlens_lib::settings::load();
    let g = Glab::from_settings(&s).unwrap();
    let me = api::current_user(&g).await.unwrap();
    let mut best = None;
    for m in api::my_mrs(&g, &me).await.unwrap() {
        let f = api::mr_diffs(&g, m.project_id, m.iid).await.unwrap();
        let size: usize = f.iter().map(|x| x.lines.len()).sum();
        if size > 0 && best.as_ref().is_none_or(|(n, _, _)| size < *n) {
            best = Some((size, m, f));
        }
    }
    let (size, m, files) = best.expect("an MR with a diff");
    let mr = api::mr_details(&g, m.project_id, m.iid).await.unwrap();
    println!("reviewing {} ({size} diff lines)", mr.references.full);
    // PEERLENS_LIVE_REPO=1 also exercises repo context (downloads the archive, read-only tools).
    let snap = if std::env::var("PEERLENS_LIVE_REPO").is_ok() {
        let sha = mr.diff_refs.as_ref().unwrap().head_sha.clone();
        let sn = peerlens_lib::review::repo::snapshot(
            &g,
            mr.source_project_id.unwrap_or(mr.project_id),
            &sha,
            300 << 20,
            &peerlens_lib::review::repo::default_exclude(),
        )
        .await
        .unwrap();
        println!("snapshot: {:?}", (sn.stats.files, sn.stats.bytes, &sn.stats.excluded));
        Some(sn)
    } else {
        None
    };
    let spec = RunSpec {
        kind: AgentKind::Claude,
        bin: peerlens_lib::platform::resolve_binary("claude", None).unwrap(),
        // PEERLENS_LIVE_MODEL=sonnet to check review quality; haiku/low keeps the smoke test cheap.
        model: Some(std::env::var("PEERLENS_LIVE_MODEL").unwrap_or_else(|_| "haiku".into())),
        effort: std::env::var("PEERLENS_LIVE_MODEL").is_err().then(|| "low".into()),
        system_prompt: prompt::system_prompt("English", AgentKind::Claude, snap.is_some(), "", ""),
        timeout: std::time::Duration::from_secs(300),
        repo: snap.as_ref().map(|s| s.root().to_path_buf()),
        known_features: None,
    };
    let (_tx, mut rx) = tokio::sync::watch::channel(false);
    let t = std::time::Instant::now();
    let emit = move |e: ReviewEvent| {
        if let ReviewEvent::Activity { text } = &e {
            println!("  activity: {text}");
        }
        if let ReviewEvent::Usage { usage } = &e {
            println!("usage: {:.0}% max, status {}", usage.max_pct(), usage.status);
        }
        if let ReviewEvent::Finding { finding: f } = &e {
            println!(
                "[{:>5.1}s] {} {}:{:?} anchored={} — {}\n         {}\n         fix: {:?}",
                t.elapsed().as_secs_f32(),
                f.severity,
                f.file,
                f.start_line,
                f.anchored,
                f.title,
                f.explanation,
                f.suggestion.as_ref().map(|s| &s.description)
            );
        }
    };
    let r = run_review(
        &spec,
        &mr,
        &files,
        None,
        &Limits {
            max_bytes: s.max_review_bytes,
            ..Limits::default()
        },
        StoredReview::default(),
        &mut rx,
        &emit,
    )
    .await;
    println!(
        "findings={} summaries={:?} errors={:?} raw_len={} skipped={:?} activity={}",
        r.findings.len(),
        r.summaries,
        r.errors,
        r.raw.len(),
        r.skipped,
        r.activity.len()
    );
    let root = snap.as_ref().map(|s| s.root().to_path_buf());
    drop(snap);
    assert!(root.is_none_or(|p| !p.exists()), "snapshot deleted");
    assert!(r.errors.is_empty());
    assert!(!r.summaries.is_empty() || !r.raw.is_empty());
}

/// Real codex binary, our exact argv, no network login needed: proves every flag and feature name
/// is accepted (codex fails on unknown flags/features before it tries to authenticate).
/// PEERLENS_CODEX_BIN=/path/to/codex cargo test --test live_glab live_codex_argv -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn live_codex_argv_accepted() {
    use peerlens_lib::agents::{runner, AgentKind};
    let bin = std::path::PathBuf::from(std::env::var("PEERLENS_CODEX_BIN").expect("PEERLENS_CODEX_BIN"));
    let probe = runner::codex_probe(&bin);
    println!(
        "codex {} logged_in={} models={:?}",
        probe.version,
        probe.logged_in,
        probe.models.iter().map(|m| &m.slug).collect::<Vec<_>>()
    );
    assert!(!probe.features.is_empty() && !probe.models.is_empty());
    for repo in [
        None,
        Some(peerlens_lib::platform::TempDir::new("peerlens-live-repo").unwrap()),
    ] {
        let spec = runner::RunSpec {
            kind: AgentKind::Codex,
            bin: bin.clone(),
            model: Some(probe.models[0].slug.clone()),
            effort: Some("low".into()),
            system_prompt: peerlens_lib::review::prompt::system_prompt(
                "English",
                AgentKind::Codex,
                repo.is_some(),
                "",
                "",
            ),
            timeout: std::time::Duration::from_secs(120),
            repo: repo.as_ref().map(|r| r.0.clone()),
            known_features: Some(probe.features.clone()),
        };
        let (_tx, mut rx) = tokio::sync::watch::channel(false);
        let res = runner::run(&spec, "Say hello.", &mut rx, |e| println!("  event: {e:?}")).await;
        println!("repo={} → {:?}", repo.is_some(), res.as_ref().err());
        if let Err(e) = res {
            let m = e.to_string();
            assert!(
                !m.contains("unexpected argument") && !m.contains("Unknown feature") && !m.contains("error parsing"),
                "argv rejected: {m}"
            );
            if !probe.logged_in {
                assert!(
                    m.contains("401") || m.to_lowercase().contains("auth") || m.contains("log"),
                    "expected an auth error, got: {m}"
                );
            }
        }
    }
}

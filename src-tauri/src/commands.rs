use crate::agents::{self, runner::RunSpec, AgentInfo, AgentKind};
use crate::diff::FileDiff;
use crate::error::Result;
use crate::gitlab::{api, models::*, runner::Glab};
use crate::review::{self, storage, ReviewEvent};
use crate::settings::{self, Settings};
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::State;
use tokio::sync::watch;

#[derive(Default)]
pub struct AppState {
    pub settings: Mutex<Settings>,
    pub me: Mutex<Option<User>>,
    /// Cancel switch of the running review; replacing it cancels the old one.
    pub cancel: Mutex<Option<watch::Sender<bool>>>,
    pub running: std::sync::atomic::AtomicBool,
    /// Start times of recent reviews, for the per-hour limit.
    pub recent: Mutex<Vec<std::time::Instant>>,
    /// Last subscription usage reported by Claude.
    pub usage: Mutex<Option<crate::agents::runner::Usage>>,
}

/// Clears the running flag even if the review errors out.
struct RunningGuard<'a>(&'a std::sync::atomic::AtomicBool);
impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Refuse to start when another review runs, the hourly cap is hit, or Claude usage is over the threshold.
fn check_can_start(st: &AppState, s: &Settings, agent: AgentKind) -> Result<()> {
    use crate::error::AppError::Agent;
    if st.running.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(Agent("A review is already running. Cancel it first.".into()));
    }
    let mut recent = st.recent.lock().unwrap();
    recent.retain(|t| t.elapsed() < std::time::Duration::from_secs(3600));
    if recent.len() >= s.max_reviews_per_hour.max(1) {
        return Err(Agent(format!(
            "Limit of {} reviews per hour reached (Settings → Usage limits).",
            s.max_reviews_per_hour
        )));
    }
    if agent == AgentKind::Claude {
        if let Some(u) = st.usage.lock().unwrap().as_ref() {
            if let Some(reset) = u.resets_at_over(s.max_usage_pct).filter(|r| *r > now_secs()) {
                let mins = (reset - now_secs()).div_ceil(60);
                return Err(Agent(format!(
                    "Claude usage is at {:.0}% (pause threshold {:.0}%). Try again in about {mins} min or raise the threshold in Settings.",
                    u.max_pct(),
                    s.max_usage_pct
                )));
            }
        }
    }
    Ok(())
}

fn glab(st: &AppState) -> Result<Glab> {
    Glab::from_settings(&st.settings.lock().unwrap())
}

#[tauri::command]
pub fn get_settings(st: State<'_, AppState>) -> Settings {
    st.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_settings(st: State<'_, AppState>, new: Settings) -> Result<()> {
    let new = new.validated().map_err(crate::error::AppError::Rejected)?;
    settings::save(&new)?;
    *st.settings.lock().unwrap() = new;
    *st.me.lock().unwrap() = None; // host may have changed
    Ok(())
}

#[derive(serde::Serialize)]
pub struct MrList {
    pub me: User,
    pub mrs: Vec<MrSummary>,
}

#[tauri::command]
pub async fn list_my_mrs(st: State<'_, AppState>) -> Result<MrList> {
    let g = glab(&st)?;
    let cached = st.me.lock().unwrap().clone();
    let me = match cached {
        Some(u) => u,
        None => {
            let u = api::current_user(&g).await?;
            *st.me.lock().unwrap() = Some(u.clone());
            u
        }
    };
    let mrs = api::my_mrs(&g, &me).await?;
    Ok(MrList { me, mrs })
}

#[tauri::command]
pub async fn mr_details(st: State<'_, AppState>, project_id: u64, iid: u64) -> Result<MrDetails> {
    api::mr_details(&glab(&st)?, project_id, iid).await
}

#[tauri::command]
pub async fn mr_commits(st: State<'_, AppState>, project_id: u64, iid: u64) -> Result<Vec<Commit>> {
    api::mr_commits(&glab(&st)?, project_id, iid).await
}

#[tauri::command]
pub async fn mr_diffs(st: State<'_, AppState>, project_id: u64, iid: u64) -> Result<Vec<FileDiff>> {
    api::mr_diffs(&glab(&st)?, project_id, iid).await
}

#[tauri::command]
pub async fn commit_diff(st: State<'_, AppState>, project_id: u64, sha: String) -> Result<Vec<FileDiff>> {
    api::commit_diff(&glab(&st)?, project_id, &sha).await
}

#[tauri::command]
pub async fn mr_discussions(st: State<'_, AppState>, project_id: u64, iid: u64) -> Result<Vec<Discussion>> {
    api::discussions(&glab(&st)?, project_id, iid).await
}

#[tauri::command]
pub fn open_url(url: String) -> Result<()> {
    Ok(crate::platform::open_url(&url)?)
}

#[tauri::command]
pub fn detect_agents(st: State<'_, AppState>) -> Vec<AgentInfo> {
    agents::detect(&st.settings.lock().unwrap())
}

#[tauri::command]
pub fn load_review(project_id: u64, iid: u64) -> Option<storage::StoredReview> {
    storage::load_latest(&storage::default_dir()?, project_id, iid)
}

#[tauri::command]
pub fn app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> Result<Option<crate::update::UpdateInfo>> {
    crate::update::check(&app).await
}

#[tauri::command]
pub async fn install_update(
    app: tauri::AppHandle,
    st: State<'_, AppState>,
    on_progress: Channel<crate::update::UpdateProgress>,
) -> Result<()> {
    if st.running.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(crate::error::AppError::Update(
            "Wait for the running review to finish (or cancel it) first.".into(),
        ));
    }
    crate::update::install(&app, &on_progress).await
}

#[tauri::command]
pub fn default_guidelines() -> &'static str {
    review::prompt::DEFAULT_GUIDELINES
}

#[tauri::command]
pub fn delete_review(st: State<'_, AppState>, project_id: u64, iid: u64) -> Result<()> {
    if st.running.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(crate::error::AppError::Agent("Cancel the running review first.".into()));
    }
    if let Some(dir) = storage::default_dir() {
        if dir.exists() {
            storage::delete(&dir, project_id, iid)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_usage(st: State<'_, AppState>) -> Option<crate::agents::runner::Usage> {
    st.usage.lock().unwrap().clone()
}

#[tauri::command]
pub fn cancel_review(st: State<'_, AppState>) {
    if let Some(tx) = st.cancel.lock().unwrap().take() {
        let _ = tx.send(true);
    }
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn start_review(
    st: State<'_, AppState>,
    project_id: u64,
    iid: u64,
    agent: AgentKind,
    model: String,
    effort: String,
    scope_file: Option<String>,
    language: String,
    repo_context: bool,
    notes: String,
    on_event: Channel<ReviewEvent>,
) -> Result<()> {
    let s = st.settings.lock().unwrap().clone();
    let g = Glab::from_settings(&s)?;
    let info = agents::detect(&s)
        .into_iter()
        .find(|a| a.kind == agent)
        .ok_or_else(|| crate::error::AppError::BinaryNotFound(format!("{agent:?}")))?;
    check_can_start(&st, &s, agent)?;
    st.running.store(true, std::sync::atomic::Ordering::SeqCst);
    let _guard = RunningGuard(&st.running);
    st.recent.lock().unwrap().push(std::time::Instant::now());
    let (tx, mut rx) = watch::channel(false);
    if let Some(old) = st.cancel.lock().unwrap().replace(tx) {
        let _ = old.send(true);
    }
    // Fetch fresh so findings are anchored to the exact head we review.
    let (mr, files) = tokio::try_join!(api::mr_details(&g, project_id, iid), api::mr_diffs(&g, project_id, iid))?;
    let head_sha = mr
        .diff_refs
        .as_ref()
        .map(|d| d.head_sha.clone())
        .or(mr.sha.clone())
        .unwrap_or_default();
    let language = if language.trim().is_empty() {
        s.review_language.clone()
    } else {
        language
    };
    // Repo context: read-only snapshot at the MR head, deleted when `snap` drops at the end of this fn.
    let snap = if repo_context {
        let _ = on_event.send(ReviewEvent::Repo {
            stage: "downloading".into(),
            stats: None,
        });
        let pid = mr.source_project_id.unwrap_or(project_id);
        let fetch = review::repo::snapshot(&g, pid, &head_sha, s.repo_max_mb.max(1) * 1_048_576, &s.repo_exclude);
        let snap = tokio::select! {
            r = fetch => r?,
            _ = rx.changed() => return Err(crate::error::AppError::Cancelled),
        };
        let _ = on_event.send(ReviewEvent::Repo {
            stage: "ready".into(),
            stats: Some(snap.stats.clone()),
        });
        Some(snap)
    } else {
        None
    };
    let spec = RunSpec {
        kind: agent,
        bin: info.path.clone().into(),
        model: Some(model.clone()).filter(|m| !m.trim().is_empty()),
        effort: Some(effort.clone()).filter(|e| match agent {
            AgentKind::Claude => crate::agents::runner::EFFORTS.contains(&e.as_str()),
            AgentKind::Codex => crate::agents::runner::CODEX_EFFORTS.contains(&e.as_str()),
        }),
        system_prompt: review::prompt::system_prompt(&language, agent, snap.is_some(), &s.review_guidelines, &notes),
        known_features: (agent == AgentKind::Codex)
            .then(|| crate::agents::runner::codex_probe(std::path::Path::new(&info.path)).features),
        timeout: std::time::Duration::from_secs(s.agent_timeout_secs.max(30)),
        repo: snap.as_ref().map(|x| x.root().to_path_buf()),
    };
    let base = storage::StoredReview {
        project_id,
        iid,
        head_sha,
        agent: format!("{agent:?}"),
        model,
        effort,
        language,
        scope: scope_file.clone().unwrap_or_else(|| "whole MR".into()),
        repo: snap.as_ref().map(|x| x.stats.clone()),
        ..Default::default()
    };
    let emit = |e: ReviewEvent| {
        if let ReviewEvent::Usage { usage } = &e {
            *st.usage.lock().unwrap() = Some(usage.clone());
        }
        let _ = on_event.send(e);
    };
    let limits = review::Limits {
        max_bytes: s.max_review_bytes,
        max_chunks: s.max_chunks,
        chunk_delay: std::time::Duration::from_secs(s.chunk_delay_secs),
        max_usage_pct: s.max_usage_pct,
    };
    let mut r = review::run_review(&spec, &mr, &files, scope_file.as_deref(), &limits, base, &mut rx, &emit).await;
    r.created_at = now_secs();
    if let Some(dir) = storage::default_dir() {
        if let Err(e) = storage::save(&dir, &r) {
            r.errors.push(format!("could not save review: {e}"));
        }
    }
    emit(ReviewEvent::Done { review: Box::new(r) });
    Ok(())
}

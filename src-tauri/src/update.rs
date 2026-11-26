//! In-app updates: the official Tauri updater, driven only from Rust (no JS permissions).
//! Releases are signed with the updater key; the public key in tauri.conf.json verifies every download.
//! Network access happens only when the user presses "Check for updates" or "Update & restart".
use crate::error::{AppError, Result};
use serde::Serialize;
use tauri::{ipc::Channel, AppHandle};
use tauri_plugin_updater::UpdaterExt;

const RELEASES: &str = "https://github.com/dvxzr/Peerlens/releases";

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub version: String,
    pub date: Option<String>,
    /// Release notes (Markdown), shown as the changelog.
    pub notes: String,
    /// False when this build can't replace itself (e.g. built from source): offer the download page instead.
    pub can_install: bool,
    pub release_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpdateProgress {
    Downloading { done: u64, total: Option<u64> },
    Installing,
}

/// macOS .app bundles always can; on Linux only the packaged formats (AppImage, .deb, .rpm).
fn can_self_update() -> bool {
    if cfg!(target_os = "linux") {
        return std::env::var_os("APPIMAGE").is_some() || tauri::utils::platform::bundle_type().is_some();
    }
    true
}

fn err(e: impl std::fmt::Display) -> AppError {
    AppError::Update(e.to_string())
}

pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>> {
    let Some(u) = app.updater().map_err(err)?.check().await.map_err(err)? else {
        return Ok(None);
    };
    Ok(Some(UpdateInfo {
        current: u.current_version.clone(),
        version: u.version.clone(),
        date: u.date.map(|d| d.date().to_string()),
        notes: u.body.clone().unwrap_or_default(),
        can_install: can_self_update(),
        release_url: format!("{RELEASES}/tag/v{}", u.version),
    }))
}

/// Download, verify the signature, install, then restart into the new version.
pub async fn install(app: &AppHandle, progress: &Channel<UpdateProgress>) -> Result<()> {
    if !can_self_update() {
        return Err(AppError::Update(
            "This build can't update itself. Download the new version instead.".into(),
        ));
    }
    let u = app
        .updater()
        .map_err(err)?
        .check()
        .await
        .map_err(err)?
        .ok_or_else(|| AppError::Update("No update available.".into()))?;
    tracing::info!(version = %u.version, "installing update");
    let mut done = 0u64;
    u.download_and_install(
        |chunk, total| {
            done += chunk as u64;
            let _ = progress.send(UpdateProgress::Downloading { done, total });
        },
        || {
            let _ = progress.send(UpdateProgress::Installing);
        },
    )
    .await
    .map_err(err)?;
    // macOS: reopen only after we have quit, so the Dock keeps its pinned icon (see platform::reopen_after_exit).
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager; // app.env()
        let bundle = tauri::process::current_binary(&app.env())
            .ok()
            .and_then(|b| b.ancestors().nth(3).map(std::path::Path::to_path_buf)) // X.app/Contents/MacOS/x
            .filter(|p| p.extension().is_some_and(|e| e == "app"));
        if let Some(bundle) = bundle {
            match crate::platform::reopen_after_exit(&bundle) {
                Ok(()) => {
                    app.exit(0);
                    return Ok(());
                }
                Err(e) => tracing::warn!(error = %e, "reopen after exit failed, restarting directly"),
            }
        }
    }
    app.restart()
}

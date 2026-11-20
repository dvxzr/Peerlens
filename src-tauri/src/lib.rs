pub mod agents;
pub mod commands;
pub mod diff;
pub mod error;
pub mod gitlab;
pub mod platform;
pub mod review;
pub mod settings;
pub mod update;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "peerlens=info".into()),
        )
        .init();
    platform::cleanup_stale_temp();
    let state = commands::AppState {
        settings: std::sync::Mutex::new(settings::load()),
        ..Default::default()
    };
    tauri::Builder::default()
        // The webview may only show the app itself; external links open in the system browser via open_url.
        .plugin(
            tauri::plugin::Builder::<tauri::Wry>::new("navigation-guard")
                .on_navigation(|_, url| {
                    let ok = matches!(url.scheme(), "tauri" | "about")
                        || matches!(url.host_str(), Some("localhost") | Some("tauri.localhost"));
                    if !ok {
                        tracing::warn!(%url, "blocked navigation");
                    }
                    ok
                })
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::list_my_mrs,
            commands::mr_details,
            commands::mr_commits,
            commands::mr_diffs,
            commands::commit_diff,
            commands::mr_discussions,
            commands::open_url,
            commands::detect_agents,
            commands::load_review,
            commands::start_review,
            commands::cancel_review,
            commands::get_usage,
            commands::delete_review,
            commands::discuss_finding,
            commands::delete_thread,
            commands::default_guidelines,
            commands::app_version,
            commands::check_update,
            commands::install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub mod parser;
pub mod runner;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Claude,
    Codex,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentInfo {
    pub kind: AgentKind,
    pub name: &'static str,
    pub path: String,
    pub version: String,
    /// None = unknown (Claude: login is checked when a review runs).
    pub logged_in: Option<bool>,
    /// Models offered by the CLI itself (Codex `debug models`); empty = use the built-in list.
    pub models: Vec<runner::AgentModel>,
}

/// Agents whose binary is found on this machine.
pub fn detect(s: &crate::settings::Settings) -> Vec<AgentInfo> {
    let opt = |p: &str| Some(p.to_string()).filter(|p| !p.is_empty());
    [
        (AgentKind::Claude, "Claude Code", "claude", opt(&s.claude_path)),
        (AgentKind::Codex, "Codex", "codex", opt(&s.codex_path)),
    ]
    .into_iter()
    .filter_map(|(kind, name, bin, ov)| {
        let p = crate::platform::resolve_binary(bin, ov.as_deref())?;
        let mut info = AgentInfo {
            kind,
            name,
            path: p.to_string_lossy().into_owned(),
            version: String::new(),
            logged_in: None,
            models: vec![],
        };
        if kind == AgentKind::Codex {
            let probe = runner::codex_probe(&p);
            (info.version, info.logged_in, info.models) = (probe.version, Some(probe.logged_in), probe.models);
        }
        Some(info)
    })
    .collect()
}

//! Runs an agent CLI read-only: empty temp dir, no tools / read-only sandbox, scrubbed env.
use super::AgentKind;
use crate::error::{AppError, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::watch;

/// Tool names denied as a second layer on top of `--tools`.
const CLAUDE_DENY: &str = "Bash Edit Write MultiEdit NotebookEdit Read Glob Grep WebFetch WebSearch Task Agent";
const CLAUDE_DENY_WITH_REPO: &str = "Bash Edit Write MultiEdit NotebookEdit WebFetch WebSearch Task Agent";
/// The only tools allowed with repo context: read-only file access, confined by `--restricted` to `--add-dir`.
const CLAUDE_READ_TOOLS: &str = "Read,Grep,Glob";

/// Reasoning effort levels accepted by `claude --effort` (2.1.328).
pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// argv for the agent. Verified against `claude --help` 2.1.328; codex NOT verified (not installed).
/// `repo`: read-only snapshot the agent may read (repo context); None = diff only, no tools at all.
/// `effort`: one of EFFORTS, anything else is ignored (CLI default).
/// Codex features turned off for reviews. Diff-only also removes command execution (`shell_tool`
/// provides exec_command); with repo context the shell stays, confined by the read-only sandbox.
pub fn codex_disabled_features(with_repo: bool) -> Vec<&'static str> {
    let mut v = vec![
        "unified_exec_tty",
        "view_image",
        "apps",
        "plugins",
        "remote_plugin",
        "browser_use",
        "browser_use_external",
        "browser_use_full_cdp_access",
        "in_app_browser",
        "computer_use",
        "image_generation",
        "hooks",
        "multi_agent",
        "goals",
        "skill_search",
        "skill_mcp_dependency_install",
        "tool_suggest",
        "sleep_tool",
    ];
    if !with_repo {
        v.extend(["shell_tool", "unified_exec", "shell_snapshot"]);
    }
    v
}

/// Effort values Codex models accept (from `codex debug models`); "ultra" delegates to sub-agents, so it's excluded.
pub const CODEX_EFFORTS: [&str; 6] = ["minimal", "low", "medium", "high", "xhigh", "max"];

/// `known_features`: names from `codex features list`; None = assume all exist (tests).
#[allow(clippy::too_many_arguments)]
pub fn build_args(
    kind: AgentKind,
    model: Option<&str>,
    effort: Option<&str>,
    system_prompt: &str,
    workdir: &Path,
    repo: Option<&Path>,
    known_features: Option<&[String]>,
) -> Vec<String> {
    let mut a: Vec<String> = match kind {
        AgentKind::Claude => {
            let mut a: Vec<String> = [
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages",
            ]
            .map(String::from)
            .to_vec();
            match repo {
                None => a.extend(["--tools", "", "--disallowedTools", CLAUDE_DENY].map(String::from)),
                Some(r) => a.extend([
                    "--tools".into(),
                    CLAUDE_READ_TOOLS.into(),
                    "--allowedTools".into(),
                    "Read Grep Glob".into(),
                    "--disallowedTools".into(),
                    CLAUDE_DENY_WITH_REPO.into(),
                    "--add-dir".into(),
                    r.to_string_lossy().into_owned(),
                ]),
            }
            a.extend(
                [
                    "--permission-mode",
                    "dontAsk",
                    "--permission-prompts",
                    "none",
                    // No hooks, plugins' skills, CLAUDE.md, MCP servers or custom agents; ignore user settings files.
                    "--safe-mode",
                    "--restricted",
                    "--strict-mcp-config",
                    "--mcp-config",
                    r#"{"mcpServers":{}}"#,
                    "--disable-slash-commands",
                    "--no-session-persistence",
                    "--system-prompt",
                    system_prompt,
                ]
                .map(String::from),
            );
            a
        }
        // shortcut: codex flags from docs, unverified here; check `codex exec --help` once installed.
        // Verified against codex-cli 0.170.0 (`codex exec --help`, `codex debug prompt-input`).
        AgentKind::Codex => {
            let mut a: Vec<String> = [
                "exec",
                "--json",
                "--ephemeral",
                "--skip-git-repo-check",
                // No MCP servers, hooks, profiles or exec-policy rules from the user's config; auth still works.
                "--ignore-user-config",
                "--ignore-rules",
                "--sandbox",
                "read-only",
            ]
            .map(String::from)
            .to_vec();
            a.extend(["--cd".into(), repo.unwrap_or(workdir).to_string_lossy().into_owned()]);
            // Our system prompt replaces Codex's default agent instructions (TOML string = JSON-escaped).
            a.extend([
                "-c".into(),
                format!(
                    "instructions={}",
                    serde_json::to_string(system_prompt).unwrap_or_default()
                ),
            ]);
            a.extend(["-c".into(), "shell_environment_policy.inherit=\"core\"".into()]);
            for f in codex_disabled_features(repo.is_some()) {
                if known_features.is_none_or(|k| k.iter().any(|x| x == f)) {
                    a.extend(["--disable".into(), f.into()]);
                }
            }
            a
        }
    };
    if let Some(m) = model.map(str::trim).filter(|m| !m.is_empty()) {
        a.push("--model".into());
        a.push(m.into());
    }
    match (kind, effort) {
        (AgentKind::Claude, Some(e)) if EFFORTS.contains(&e) => a.extend(["--effort".into(), e.into()]),
        (AgentKind::Codex, Some(e)) if CODEX_EFFORTS.contains(&e) => {
            a.extend(["-c".into(), format!("model_reasoning_effort=\"{e}\"")])
        }
        _ => {}
    }
    if kind == AgentKind::Codex {
        a.push("-".into()); // prompt from stdin
    }
    a
}

/// Remove Git/GitLab/GitHub credentials and CI tokens from the child environment.
pub fn scrub_env(vars: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    const PREFIXES: [&str; 6] = ["GITLAB_", "GLAB_", "GITHUB_", "GH_", "GIT_", "CI_"];
    const EXACT: [&str; 4] = ["SSH_AUTH_SOCK", "SSH_AGENT_PID", "GL_TOKEN", "NPM_TOKEN"];
    vars.filter(|(k, _)| {
        let u = k.to_ascii_uppercase();
        !PREFIXES.iter().any(|p| u.starts_with(p)) && !EXACT.contains(&u.as_str())
    })
    .collect()
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct UsageWindow {
    pub name: String,
    /// 0.0 – 1.0
    pub utilization: f64,
    pub resets_at: u64,
    pub status: String,
}

/// Subscription usage as reported by `claude` in its `rate_limit_event`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Usage {
    pub status: String,
    pub windows: Vec<UsageWindow>,
}

impl Usage {
    pub fn max_pct(&self) -> f64 {
        self.windows.iter().map(|w| w.utilization * 100.0).fold(0.0, f64::max)
    }
    pub fn blocked(&self) -> bool {
        self.status != "allowed" || self.windows.iter().any(|w| w.status != "allowed")
    }
    /// Latest reset time among windows that are over `pct`.
    pub fn resets_at_over(&self, pct: f64) -> Option<u64> {
        self.windows
            .iter()
            .filter(|w| w.utilization * 100.0 >= pct || w.status != "allowed")
            .map(|w| w.resets_at)
            .max()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    Text(String),
    Usage(Usage),
    /// A tool call by the agent, e.g. ("Read", "/tmp/…/src/a.py").
    Tool(String, String),
    /// A fatal error reported by the agent itself (e.g. codex `turn.failed`).
    Error(String),
}

fn tool_detail(input: &serde_json::Value) -> String {
    let g = |k: &str| input.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let (pat, path) = (g("pattern"), g("path"));
    match (g("file_path"), g("command")) {
        (f, _) if !f.is_empty() => f.to_string(),
        (_, c) if !c.is_empty() => c.to_string(),
        _ if !pat.is_empty() && !path.is_empty() => format!("{pat}  in {path}"),
        _ => format!("{pat}{path}"),
    }
}

fn parse_usage(v: &serde_json::Value) -> Option<Usage> {
    let info = v.get("rate_limit_info")?;
    let mut windows: Vec<UsageWindow> = info
        .get("unifiedWindows")
        .and_then(|w| w.as_object())
        .map(|m| {
            m.iter()
                .map(|(k, w)| UsageWindow {
                    name: k.clone(),
                    utilization: w.get("utilization").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    resets_at: w.get("resetsAt").and_then(|x| x.as_u64()).unwrap_or(0),
                    status: w.get("status").and_then(|x| x.as_str()).unwrap_or("allowed").into(),
                })
                .collect()
        })
        .unwrap_or_default();
    windows.sort_by(|a, b| a.name.cmp(&b.name));
    Some(Usage {
        status: info.get("status").and_then(|x| x.as_str()).unwrap_or("allowed").into(),
        windows,
    })
}

/// Pull assistant text (and usage info) out of one stdout line of the CLI's JSON event stream.
#[derive(Default)]
pub struct EventText {
    got_delta: bool,
    /// Last error message the CLI reported in its event stream (shown instead of raw stderr).
    pub last_error: Option<String>,
}

impl EventText {
    pub fn events(&mut self, kind: AgentKind, line: &str) -> Vec<AgentEvent> {
        let quick = |k: &str| line.contains(k);
        if kind == AgentKind::Claude && (quick("\"rate_limit_event\"") || quick("\"tool_use\"")) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                return vec![];
            };
            match v.get("type").and_then(|t| t.as_str()) {
                Some("rate_limit_event") => return parse_usage(&v).map(AgentEvent::Usage).into_iter().collect(),
                Some("assistant") => {
                    let content = v
                        .pointer("/message/content")
                        .and_then(|c| c.as_array())
                        .cloned()
                        .unwrap_or_default();
                    return content
                        .iter()
                        .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
                        .map(|c| {
                            AgentEvent::Tool(
                                c.get("name").and_then(|n| n.as_str()).unwrap_or("tool").into(),
                                tool_detail(c.get("input").unwrap_or(&serde_json::Value::Null)),
                            )
                        })
                        .collect();
                }
                _ => {}
            }
        }
        if kind == AgentKind::Codex && (quick("command_execution") || quick("error") || quick("turn.failed")) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                let s = |p: &str| v.pointer(p).and_then(|x| x.as_str());
                match (s("/type"), s("/item/type")) {
                    (Some("item.started"), Some("command_execution")) => {
                        return s("/item/command")
                            .map(|c| vec![AgentEvent::Tool("shell".into(), c.into())])
                            .unwrap_or_default();
                    }
                    (Some("item.completed" | "item.updated"), Some("command_execution")) => return vec![],
                    (Some("error"), _) | (Some("item.completed"), Some("error")) => {
                        self.last_error = s("/message").or(s("/item/message")).map(String::from);
                        return vec![];
                    }
                    (Some("turn.failed"), _) => {
                        let m = s("/error/message")
                            .map(String::from)
                            .or(self.last_error.clone())
                            .unwrap_or_else(|| "turn failed".into());
                        self.last_error = Some(m.clone());
                        return vec![AgentEvent::Error(m)];
                    }
                    _ => {}
                }
            }
        }
        self.extract(kind, line).map(AgentEvent::Text).into_iter().collect()
    }

    #[cfg(test)]
    pub fn event(&mut self, kind: AgentKind, line: &str) -> Option<AgentEvent> {
        self.events(kind, line).into_iter().next()
    }

    pub fn extract(&mut self, kind: AgentKind, line: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(line).ok()?;
        let s = |p: &str| v.pointer(p).and_then(|x| x.as_str()).map(String::from);
        match kind {
            AgentKind::Claude => match v.get("type")?.as_str()? {
                "stream_event" if s("/event/delta/type").as_deref() == Some("text_delta") => {
                    self.got_delta = true;
                    s("/event/delta/text")
                }
                // Full result only if no deltas arrived (older CLIs).
                "result" if !self.got_delta => s("/result"),
                _ => None,
            },
            AgentKind::Codex => {
                // New format: item.completed {item:{type:agent_message,text}}; old: {msg:{type:agent_message,message}}
                let t = s("/item/type").or_else(|| s("/msg/type"))?;
                (t == "agent_message")
                    .then(|| s("/item/text").or_else(|| s("/msg/message")))
                    .flatten()
                    .map(|t| t + "\n")
            }
        }
    }
}

pub struct RunSpec {
    pub kind: AgentKind,
    pub bin: PathBuf,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub system_prompt: String,
    pub timeout: Duration,
    /// Read-only repository snapshot the agent may read; None = diff only.
    pub repo: Option<PathBuf>,
    /// Codex only: feature names this codex version knows (from `codex features list`).
    pub known_features: Option<Vec<String>>,
}

/// Where Codex keeps auth: $CODEX_HOME or ~/.codex.
pub fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".codex"))
}

pub struct RunOutput {
    pub stderr: String,
}

/// Run the agent once; `on_text` receives assistant text as it streams.
pub async fn run(
    spec: &RunSpec,
    prompt: &str,
    cancel: &mut watch::Receiver<bool>,
    mut on_event: impl FnMut(AgentEvent),
) -> Result<RunOutput> {
    let dir = crate::platform::TempDir::new("peerlens-agent")?;
    let args = build_args(
        spec.kind,
        spec.model.as_deref(),
        spec.effort.as_deref(),
        &spec.system_prompt,
        &dir.0,
        spec.repo.as_deref(),
        spec.known_features.as_deref(),
    );
    tracing::info!(target: "peerlens::agent", bin = ?spec.bin, cwd = ?dir.0,
        argv = ?args.iter().map(|a| if a.len() > 80 { "<system prompt>" } else { a }).collect::<Vec<_>>(), "agent spawn");
    let mut cmd = tokio::process::Command::new(&spec.bin);
    cmd.args(&args)
        .current_dir(&dir.0)
        .env_clear()
        .envs(scrub_env(std::env::vars()))
        .env("PATH", crate::platform::child_path())
        // Defense in depth: even if a tool ran, git/glab/gh would find no config or credentials.
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GLAB_CONFIG_DIR", &dir.0)
        .env("GH_CONFIG_DIR", &dir.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if spec.kind == AgentKind::Codex {
        // Commands Codex runs see an empty HOME (no ~/.ssh, ~/.gitconfig …); auth stays in CODEX_HOME.
        cmd.env("CODEX_HOME", codex_home()).env("HOME", &dir.0);
    }
    crate::platform::new_process_group(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::Agent(format!("spawn {}: {e}", spec.bin.display())))?;
    let pid = child.id().unwrap_or(0);

    let mut stdin = child.stdin.take().unwrap();
    let prompt = prompt.to_owned();
    let writer = tokio::spawn(async move {
        let _ = stdin.write_all(prompt.as_bytes()).await;
        let _ = stdin.shutdown().await; // close → EOF for the CLI
    });
    let mut stderr = child.stderr.take().unwrap();
    let err_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = (&mut stderr).take(64 * 1024).read_to_end(&mut buf).await;
        String::from_utf8_lossy(&buf).into_owned()
    });

    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut ev = EventText::default();
    let deadline = tokio::time::sleep(spec.timeout);
    tokio::pin!(deadline);
    let outcome: Result<()> = loop {
        tokio::select! {
            l = lines.next_line() => match l {
                Ok(Some(l)) => for e in ev.events(spec.kind, &l) { on_event(e) },
                Ok(None) => break Ok(()),
                Err(e) => break Err(e.into()),
            },
            _ = cancel.changed() => break Err(AppError::Cancelled),
            _ = &mut deadline => break Err(AppError::Agent(format!("timed out after {}s", spec.timeout.as_secs()))),
        }
    };
    if outcome.is_err() {
        crate::platform::kill_tree(pid);
        let _ = child.kill().await;
    }
    writer.abort();
    let status = child.wait().await?;
    let stderr = err_task.await.unwrap_or_default();
    outcome?;
    if !status.success() {
        // Prefer the agent's own error message over its stderr log noise.
        let tail: Vec<&str> = stderr.trim().lines().rev().take(6).collect();
        let detail = ev
            .last_error
            .clone()
            .unwrap_or_else(|| tail.into_iter().rev().collect::<Vec<_>>().join("\n"));
        return Err(AppError::Agent(format!("exited with {status}: {detail}")));
    }
    Ok(RunOutput { stderr })
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AgentModel {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub efforts: Vec<String>,
    pub default_effort: String,
}

/// What the installed Codex supports. Local commands only; no network, no writes.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct CodexProbe {
    pub version: String,
    pub logged_in: bool,
    pub models: Vec<AgentModel>,
    #[serde(skip)]
    pub features: Vec<String>,
}

pub fn parse_codex_models(json: &str) -> Vec<AgentModel> {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let list = v.get("models").unwrap_or(&v).as_array().cloned().unwrap_or_default();
    list.iter()
        .filter(|m| m.get("visibility").and_then(|x| x.as_str()).is_none_or(|x| x == "list"))
        .filter_map(|m| {
            let g = |k: &str| m.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let slug = g("slug");
            (!slug.is_empty()).then(|| AgentModel {
                name: Some(g("display_name"))
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| slug.clone()),
                slug,
                description: g("description"),
                efforts: m
                    .get("supported_reasoning_levels")
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|e| e.get("effort").and_then(|x| x.as_str()))
                            .filter(|e| CODEX_EFFORTS.contains(e))
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default(),
                default_effort: g("default_reasoning_level"),
            })
        })
        .collect()
}

pub fn parse_codex_features(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let (name, stage) = (it.next()?, it.next()?);
            (stage != "removed" && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                .then(|| name.to_string())
        })
        .collect()
}

/// Run `codex --version`, `login status`, `features list` and `debug models` once per binary.
pub fn codex_probe(bin: &Path) -> CodexProbe {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CodexProbe>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let run = |args: &[&str]| {
        std::process::Command::new(bin)
            .args(args)
            .current_dir(std::env::temp_dir())
            .env_clear()
            .envs(scrub_env(std::env::vars()))
            .env("PATH", crate::platform::child_path())
            .env("CODEX_HOME", codex_home())
            .stdin(Stdio::null())
            .output()
            .ok()
    };
    let text = |o: &Option<std::process::Output>| {
        o.as_ref()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    let logged_in = run(&["login", "status"]).is_some_and(|o| o.status.success());
    if let Some(p) = cache.lock().unwrap().get_mut(bin) {
        p.logged_in = logged_in;
        return p.clone();
    }
    let p = CodexProbe {
        version: text(&run(&["--version"])).trim().to_string(),
        logged_in,
        models: parse_codex_models(&text(&run(&["debug", "models"]))),
        features: parse_codex_features(&text(&run(&["features", "list"]))),
    };
    tracing::info!(version = %p.version, logged_in = p.logged_in, models = p.models.len(), "codex probe");
    cache.lock().unwrap().insert(bin.to_path_buf(), p.clone());
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_argv_is_read_only() {
        let a = build_args(
            AgentKind::Claude,
            Some("opus"),
            Some("high"),
            "SYS",
            Path::new("/tmp/x"),
            None,
            None,
        );
        let has = |x: &str| a.iter().any(|y| y == x);
        let after = |x: &str| a[a.iter().position(|y| y == x).unwrap() + 1].clone();
        assert!(
            has("-p")
                && has("--safe-mode")
                && has("--restricted")
                && has("--strict-mcp-config")
                && has("--no-session-persistence")
        );
        assert_eq!(after("--tools"), "");
        assert_eq!(after("--permission-mode"), "dontAsk");
        assert_eq!(after("--permission-prompts"), "none");
        assert!(after("--disallowedTools").contains("Bash") && after("--disallowedTools").contains("Edit"));
        assert_eq!(after("--model"), "opus");
        assert_eq!(after("--effort"), "high");
        let bad = build_args(
            AgentKind::Claude,
            None,
            Some("--dangerously-skip-permissions"),
            "SYS",
            Path::new("/tmp/x"),
            None,
            None,
        );
        assert!(
            !bad.iter().any(|x| x == "--effort" || x.contains("dangerously")),
            "unknown effort is dropped"
        );
        for bad in [
            "--dangerously-skip-permissions",
            "bypassPermissions",
            "acceptEdits",
            "--add-dir",
            "--allowedTools",
        ] {
            assert!(!has(bad), "{bad}");
        }
    }

    #[test]
    fn claude_repo_mode_allows_only_read_tools() {
        let a = build_args(
            AgentKind::Claude,
            None,
            None,
            "SYS",
            Path::new("/tmp/x"),
            Some(Path::new("/tmp/repo")),
            None,
        );
        let after = |x: &str| a[a.iter().position(|y| y == x).unwrap() + 1].clone();
        assert_eq!(after("--tools"), "Read,Grep,Glob");
        assert_eq!(after("--allowedTools"), "Read Grep Glob");
        assert_eq!(after("--add-dir"), "/tmp/repo");
        let deny = after("--disallowedTools");
        for t in [
            "Bash",
            "Edit",
            "Write",
            "MultiEdit",
            "NotebookEdit",
            "WebFetch",
            "WebSearch",
            "Task",
        ] {
            assert!(deny.split(' ').any(|d| d == t), "{t} must be denied");
        }
        assert!(a.iter().any(|x| x == "--restricted") && a.iter().any(|x| x == "--safe-mode"));
        assert_eq!(after("--permission-mode"), "dontAsk");
    }

    #[test]
    fn parses_tool_use() {
        let l = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"x"},{"type":"tool_use","name":"Read","input":{"file_path":"/r/a.py"}},{"type":"tool_use","name":"Grep","input":{"pattern":"def x","path":"/r"}}]}}"#;
        let e = EventText::default().events(AgentKind::Claude, l);
        assert_eq!(
            e,
            vec![
                AgentEvent::Tool("Read".into(), "/r/a.py".into()),
                AgentEvent::Tool("Grep".into(), "def x  in /r".into())
            ]
        );
    }

    #[test]
    fn codex_argv_is_read_only() {
        let a = build_args(
            AgentKind::Codex,
            Some("gpt-x"),
            Some("high"),
            "SYS \"q\"\nline",
            Path::new("/tmp/x"),
            None,
            None,
        );
        let s = a.join(" ");
        assert!(s.starts_with("exec --json --ephemeral --skip-git-repo-check --ignore-user-config --ignore-rules --sandbox read-only --cd /tmp/x "), "{s}");
        assert!(
            a.contains(&r#"instructions="SYS \"q\"\nline""#.to_string()),
            "system prompt as TOML string: {s}"
        );
        for f in [
            "shell_tool",
            "unified_exec",
            "hooks",
            "multi_agent",
            "computer_use",
            "browser_use",
        ] {
            assert!(s.contains(&format!("--disable {f}")), "{f} must be disabled");
        }
        assert!(s.contains(r#"model_reasoning_effort="high""#) && s.contains("--model gpt-x") && s.ends_with(" -"));
        for bad in [
            "workspace-write",
            "danger-full-access",
            "--full-auto",
            "--dangerously-bypass",
            "--add-dir",
            "--trust",
            "--approve-for-me",
        ] {
            assert!(!s.contains(bad), "{bad}");
        }
        // Repo context keeps the shell (read-only sandbox) and points --cd at the snapshot.
        let r = build_args(
            AgentKind::Codex,
            None,
            Some("ultra"),
            "S",
            Path::new("/tmp/x"),
            Some(Path::new("/tmp/repo")),
            None,
        )
        .join(" ");
        assert!(r.contains("--cd /tmp/repo") && !r.contains("--disable shell_tool") && r.contains("--disable hooks"));
        assert!(!r.contains("ultra"), "sub-agent delegation effort is not allowed");
        // Only features this codex knows are passed (unknown names make codex exit).
        let known = vec!["hooks".to_string()];
        let k = build_args(
            AgentKind::Codex,
            None,
            None,
            "S",
            Path::new("/tmp/x"),
            None,
            Some(&known),
        )
        .join(" ");
        assert!(k.contains("--disable hooks") && !k.contains("--disable shell_tool"));
    }

    #[test]
    fn parses_codex_probe_output() {
        let m = parse_codex_models(
            r#"{"models":[{"slug":"gpt-a","display_name":"GPT-A","description":"d","default_reasoning_level":"low","supported_reasoning_levels":[{"effort":"low"},{"effort":"ultra"}],"visibility":"list"},{"slug":"hidden","visibility":"hide"}]}"#,
        );
        assert_eq!(m.len(), 1);
        assert_eq!(
            (m[0].name.as_str(), m[0].efforts.clone()),
            ("GPT-A", vec!["low".to_string()])
        );
        let f = parse_codex_features("shell_tool   stable  true\nold_thing  removed  false\n\nhooks stable true\n");
        assert_eq!(f, ["shell_tool", "hooks"]);
    }

    #[test]
    fn codex_error_events() {
        let mut e = EventText::default();
        assert!(e
            .events(
                AgentKind::Codex,
                r#"{"type":"error","message":"Reconnecting... 1/5 (401 Unauthorized)"}"#
            )
            .is_empty());
        assert_eq!(
            e.events(
                AgentKind::Codex,
                r#"{"type":"turn.failed","error":{"message":"401 Unauthorized"}}"#
            ),
            vec![AgentEvent::Error("401 Unauthorized".into())]
        );
        assert_eq!(
            e.events(
                AgentKind::Codex,
                r#"{"type":"item.started","item":{"type":"command_execution","command":"rg foo"}}"#
            ),
            vec![AgentEvent::Tool("shell".into(), "rg foo".into())]
        );
        assert!(e
            .events(
                AgentKind::Codex,
                r#"{"type":"item.completed","item":{"type":"command_execution","command":"rg foo"}}"#
            )
            .is_empty());
    }

    #[test]
    fn scrubs_credentials() {
        let env = [
            "GITLAB_TOKEN",
            "GLAB_TOKEN",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "GIT_ASKPASS",
            "CI_JOB_TOKEN",
            "SSH_AUTH_SOCK",
            "HOME",
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "PATH",
        ]
        .map(|k| (k.to_string(), "v".to_string()));
        let kept: Vec<String> = scrub_env(env.into_iter()).into_iter().map(|(k, _)| k).collect();
        assert_eq!(kept, ["HOME", "ANTHROPIC_API_KEY", "OPENAI_API_KEY", "PATH"]);
    }

    #[test]
    fn parses_rate_limit_event() {
        let l = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1,"unifiedWindows":{"five_hour":{"utilization":0.05,"resetsAt":100,"status":"allowed"},"seven_day":{"utilization":0.85,"resetsAt":200,"status":"allowed"}}}}"#;
        let Some(AgentEvent::Usage(u)) = EventText::default().event(AgentKind::Claude, l) else {
            panic!()
        };
        assert_eq!(u.windows.len(), 2);
        assert!((u.max_pct() - 85.0).abs() < 1e-9);
        assert!(!u.blocked());
        assert_eq!(u.resets_at_over(80.0), Some(200));
        assert_eq!(u.resets_at_over(90.0), None);
        let rejected = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected"}}"#;
        let Some(AgentEvent::Usage(u)) = EventText::default().event(AgentKind::Claude, rejected) else {
            panic!()
        };
        assert!(u.blocked());
    }

    #[test]
    fn extracts_text_from_both_clis() {
        let mut e = EventText::default();
        assert_eq!(e.extract(AgentKind::Claude, r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"ab"}}}"#).as_deref(), Some("ab"));
        assert_eq!(
            e.extract(AgentKind::Claude, r#"{"type":"result","result":"ab"}"#),
            None,
            "no double text after deltas"
        );
        assert_eq!(
            EventText::default()
                .extract(AgentKind::Claude, r#"{"type":"result","result":"ab"}"#)
                .as_deref(),
            Some("ab")
        );
        assert_eq!(
            e.extract(
                AgentKind::Codex,
                r#"{"type":"item.completed","item":{"type":"agent_message","text":"x"}}"#
            )
            .as_deref(),
            Some("x\n")
        );
        assert_eq!(
            e.extract(AgentKind::Codex, r#"{"msg":{"type":"agent_message","message":"y"}}"#)
                .as_deref(),
            Some("y\n")
        );
        assert_eq!(e.extract(AgentKind::Codex, "not json"), None);
    }
}

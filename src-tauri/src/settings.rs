use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Explicit glab binary path; empty = auto-detect.
    pub glab_path: String,
    /// GitLab host passed as `--hostname`; empty = glab's default host.
    pub host: String,
    /// Group full path (e.g. "team/sub") whose open MRs the sidebar's "All" shows; empty = all my top-level groups.
    pub mr_group: String,
    pub claude_path: String,
    pub codex_path: String,
    pub default_agent: String,
    pub review_language: String,
    /// Above this many bytes of diff the review runs file by file.
    pub max_review_bytes: usize,
    /// Files whose diff is larger than this are collapsed in the UI.
    pub collapse_lines: usize,
    pub agent_timeout_secs: u64,
    /// "dark" | "light" | "system"
    pub theme: String,
    /// Model alias preselected in the review panel ("" = CLI default).
    pub default_model: String,
    /// "" = CLI default, else one of agents::runner::EFFORTS.
    pub default_effort: String,
    /// Codex model slug ("" = Codex's own default).
    pub default_codex_model: String,
    /// Pause between per-file agent runs of one review.
    pub chunk_delay_secs: u64,
    /// Refuse reviews that would need more agent runs than this.
    pub max_chunks: usize,
    pub max_reviews_per_hour: usize,
    /// Pause reviews when any Claude usage window reaches this percent.
    pub max_usage_pct: f64,
    /// Custom review guidelines; "" = built-in default (review::prompt::DEFAULT_GUIDELINES).
    pub review_guidelines: String,
    /// Repo context: max archive size to download.
    pub repo_max_mb: u64,
    /// Repo context: file-name globs never unpacked (secrets).
    pub repo_exclude: Vec<String>,
    /// Repo context: disk budget for snapshots kept for later reviews and discussions.
    pub repo_cache_mb: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            glab_path: String::new(),
            host: String::new(),
            mr_group: String::new(),
            claude_path: String::new(),
            codex_path: String::new(),
            default_agent: "claude".into(),
            review_language: "English".into(),
            max_review_bytes: 120_000,
            collapse_lines: 800,
            agent_timeout_secs: 900,
            theme: "dark".into(),
            default_model: "sonnet".into(),
            default_effort: String::new(),
            default_codex_model: String::new(),
            chunk_delay_secs: 3,
            max_chunks: 25,
            max_reviews_per_hour: 20,
            max_usage_pct: 80.0,
            review_guidelines: String::new(),
            repo_max_mb: 300,
            repo_exclude: crate::review::repo::default_exclude(),
            repo_cache_mb: 2_048,
        }
    }
}

impl Settings {
    /// Validate values coming from the UI (trust boundary) before they are saved or used.
    pub fn validated(mut self) -> Result<Self, String> {
        for (name, value) in [
            ("glab", &self.glab_path),
            ("claude", &self.claude_path),
            ("codex", &self.codex_path),
        ] {
            check_binary(name, value)?;
        }
        let host_ok =
            |h: &str| h.chars().all(|c| c.is_ascii_alphanumeric() || ".-:".contains(c)) && !h.starts_with('-');
        if !host_ok(self.host.trim()) {
            return Err(format!("Invalid GitLab host: {}", self.host));
        }
        self.host = self.host.trim().to_string();
        let group = self.mr_group.trim().trim_matches('/');
        let group_ok = group.chars().all(|c| c.is_ascii_alphanumeric() || "_-./".contains(c))
            && !group.contains("..")
            && !group.starts_with(['-', '.']);
        if !group_ok {
            return Err(format!("Invalid GitLab group path: {}", self.mr_group));
        }
        self.mr_group = group.to_string();
        if !["dark", "light", "system"].contains(&self.theme.as_str()) {
            self.theme = "dark".into();
        }
        if !["claude", "codex"].contains(&self.default_agent.as_str()) {
            self.default_agent = "claude".into();
        }
        self.review_language = self.review_language.trim().chars().take(40).collect();
        if self.review_language.is_empty() {
            self.review_language = "English".into();
        }
        self.review_guidelines = self.review_guidelines.chars().take(20_000).collect();
        self.repo_exclude.retain(|p| !p.trim().is_empty());
        self.repo_exclude.truncate(200);
        self.max_review_bytes = self.max_review_bytes.clamp(1_000, 5_000_000);
        self.collapse_lines = self.collapse_lines.clamp(50, 100_000);
        self.agent_timeout_secs = self.agent_timeout_secs.clamp(30, 7_200);
        self.chunk_delay_secs = self.chunk_delay_secs.min(600);
        self.max_chunks = self.max_chunks.clamp(1, 500);
        self.max_reviews_per_hour = self.max_reviews_per_hour.clamp(1, 1_000);
        self.max_usage_pct = self.max_usage_pct.clamp(1.0, 100.0);
        self.repo_max_mb = self.repo_max_mb.clamp(1, 5_000);
        self.repo_cache_mb = self.repo_cache_mb.clamp(100, 100_000);
        Ok(self)
    }
}

/// A binary override must be an absolute path to an executable whose file name starts with the tool's name.
fn check_binary(name: &str, value: &str) -> Result<(), String> {
    let v = value.trim();
    if v.is_empty() {
        return Ok(());
    }
    let p = std::path::Path::new(v);
    let file = p
        .file_name()
        .map(|f| f.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !p.is_absolute() || !file.starts_with(name) || !p.is_file() {
        return Err(format!(
            "{name} path must be an absolute path to the {name} executable: {v}"
        ));
    }
    Ok(())
}

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("peerlens").join("settings.json"))
}

pub fn load() -> Settings {
    path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
        .and_then(|s| s.validated().ok())
        .unwrap_or_default()
}

pub fn save(s: &Settings) -> crate::error::Result<()> {
    let p = path().ok_or_else(|| std::io::Error::other("no config dir"))?;
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(p, serde_json::to_vec_pretty(s)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_ui_input() {
        let ok = Settings {
            max_usage_pct: 500.0,
            max_chunks: 0,
            theme: "neon".into(),
            ..Default::default()
        }
        .validated()
        .unwrap();
        assert_eq!((ok.max_usage_pct, ok.max_chunks, ok.theme.as_str()), (100.0, 1, "dark"));
        for bad in ["/bin/sh", "claude", "../claude", "/nonexistent/claude"] {
            assert!(
                Settings {
                    claude_path: bad.into(),
                    ..Default::default()
                }
                .validated()
                .is_err(),
                "{bad}"
            );
        }
        assert!(Settings {
            host: "--evil".into(),
            ..Default::default()
        }
        .validated()
        .is_err());
        for bad in ["../x", "a b", "-x", "a?b=1"] {
            assert!(
                Settings {
                    mr_group: bad.into(),
                    ..Default::default()
                }
                .validated()
                .is_err(),
                "{bad}"
            );
        }
        let g = Settings {
            mr_group: " /team/sub-1/ ".into(),
            ..Default::default()
        }
        .validated()
        .unwrap();
        assert_eq!(g.mr_group, "team/sub-1");
        assert!(Settings {
            host: "gitlab.example.com:8443".into(),
            ..Default::default()
        }
        .validated()
        .is_ok());
        let d = crate::platform::TempDir::new("peerlens-settings").unwrap();
        let glab = d.0.join("glab");
        std::fs::write(&glab, "").unwrap();
        let custom = Settings {
            glab_path: glab.to_string_lossy().into(),
            ..Default::default()
        };
        assert!(custom.validated().is_ok());
    }
}

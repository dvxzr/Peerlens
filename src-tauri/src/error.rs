use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0} not found. Install it or set its path in Settings.")]
    BinaryNotFound(String),
    #[error("Rejected GitLab request: {0}")]
    Rejected(String),
    #[error("GitLab: HTTP 404 for {0}")]
    NotFound(String),
    #[error("glab failed (exit {code}): {stderr}")]
    Glab { code: i32, stderr: String },
    #[error("Agent: {0}")]
    Agent(String),
    #[error("Cancelled")]
    Cancelled,
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

// Tauri commands return errors to JS as plain strings.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

//! Only the fields the UI uses; everything else in GitLab responses is ignored.
use serde::{Deserialize, Deserializer, Serialize};

/// GitLab sometimes sends `null` where we expect a value; treat it as the default.
fn nd<'de, D: Deserializer<'de>, T: Default + Deserialize<'de>>(d: D) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct User {
    #[serde(default, deserialize_with = "nd")]
    pub id: u64,
    #[serde(default, deserialize_with = "nd")]
    pub username: String,
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct References {
    #[serde(default, deserialize_with = "nd")]
    pub full: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrSummary {
    pub project_id: u64,
    pub iid: u64,
    pub title: String,
    pub web_url: String,
    pub author: User,
    pub source_branch: String,
    pub target_branch: String,
    pub updated_at: String,
    #[serde(default, deserialize_with = "nd")]
    pub draft: bool,
    #[serde(default)]
    pub sha: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub references: References,
    #[serde(default, deserialize_with = "nd")]
    pub user_notes_count: u64,
    /// Filled by us, not by GitLab.
    #[serde(default, deserialize_with = "nd")]
    pub is_assignee: bool,
    #[serde(default, deserialize_with = "nd")]
    pub is_reviewer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Pipeline {
    #[serde(default, deserialize_with = "nd")]
    pub status: String,
    #[serde(default, deserialize_with = "nd")]
    pub web_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiffRefs {
    #[serde(default, deserialize_with = "nd")]
    pub base_sha: String,
    #[serde(default, deserialize_with = "nd")]
    pub head_sha: String,
    #[serde(default, deserialize_with = "nd")]
    pub start_sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrDetails {
    pub project_id: u64,
    /// Differs from project_id for MRs from forks.
    #[serde(default)]
    pub source_project_id: Option<u64>,
    pub iid: u64,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub state: String,
    pub web_url: String,
    pub author: User,
    pub source_branch: String,
    pub target_branch: String,
    #[serde(default, deserialize_with = "nd")]
    pub draft: bool,
    #[serde(default)]
    pub detailed_merge_status: Option<String>,
    #[serde(default)]
    pub head_pipeline: Option<Pipeline>,
    #[serde(default)]
    pub diff_refs: Option<DiffRefs>,
    #[serde(default)]
    pub sha: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub references: References,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    pub id: String,
    pub short_id: String,
    pub title: String,
    #[serde(default, deserialize_with = "nd")]
    pub author_name: String,
    #[serde(default, deserialize_with = "nd")]
    pub authored_date: String,
    #[serde(default, deserialize_with = "nd")]
    pub web_url: String,
}

/// Raw file diff as GitLab returns it from /diffs, /changes and commit /diff.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawDiff {
    #[serde(default, deserialize_with = "nd")]
    pub old_path: String,
    #[serde(default, deserialize_with = "nd")]
    pub new_path: String,
    #[serde(default, deserialize_with = "nd")]
    pub diff: String,
    #[serde(default, deserialize_with = "nd")]
    pub new_file: bool,
    #[serde(default, deserialize_with = "nd")]
    pub renamed_file: bool,
    #[serde(default, deserialize_with = "nd")]
    pub deleted_file: bool,
    #[serde(default, deserialize_with = "nd")]
    pub collapsed: bool,
    #[serde(default, deserialize_with = "nd")]
    pub too_large: bool,
    #[serde(default, deserialize_with = "nd")]
    pub generated_file: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Position {
    #[serde(default)]
    pub new_path: Option<String>,
    #[serde(default)]
    pub old_path: Option<String>,
    #[serde(default)]
    pub new_line: Option<u32>,
    #[serde(default)]
    pub old_line: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: u64,
    #[serde(default, deserialize_with = "nd")]
    pub body: String,
    #[serde(default, deserialize_with = "nd")]
    pub author: User,
    #[serde(default, deserialize_with = "nd")]
    pub created_at: String,
    #[serde(default, deserialize_with = "nd")]
    pub system: bool,
    #[serde(default, deserialize_with = "nd")]
    pub resolvable: bool,
    #[serde(default, deserialize_with = "nd")]
    pub resolved: bool,
    #[serde(default)]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discussion {
    pub id: String,
    #[serde(default, deserialize_with = "nd")]
    pub individual_note: bool,
    #[serde(default, deserialize_with = "nd")]
    pub notes: Vec<Note>,
}

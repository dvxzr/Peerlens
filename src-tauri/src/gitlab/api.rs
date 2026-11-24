//! GitLab endpoints, all read-only via `Glab::get`.
use super::models::*;
use super::runner::Glab;
use crate::diff::{self, FileDiff};
use crate::error::{AppError, Result};
use serde::de::DeserializeOwned;

async fn get<T: DeserializeOwned>(g: &Glab, ep: &str, paginate: bool) -> Result<T> {
    Ok(serde_json::from_value(g.get(ep, paginate).await?)?)
}

fn mr_path(project_id: u64, iid: u64) -> String {
    format!("projects/{project_id}/merge_requests/{iid}")
}

pub async fn current_user(g: &Glab) -> Result<User> {
    get(g, "user", false).await
}

/// Open MRs assigned to me ∪ open MRs where I'm a reviewer, deduped by (project, iid).
pub async fn my_mrs(g: &Glab, me: &User) -> Result<Vec<MrSummary>> {
    let assigned_ep = "merge_requests?scope=assigned_to_me&state=opened&per_page=100";
    let reviewer_ep = format!(
        "merge_requests?scope=all&state=opened&reviewer_username={}&per_page=100",
        me.username
    );
    let (assigned, reviewing) = tokio::try_join!(
        get::<Vec<MrSummary>>(g, assigned_ep, true),
        get::<Vec<MrSummary>>(g, &reviewer_ep, true)
    )?;
    Ok(merge_roles(assigned, reviewing))
}

/// Mine (with role flags) plus every other open MR visible to me.
pub async fn all_mrs(g: &Glab, me: &User) -> Result<Vec<MrSummary>> {
    // shortcut: one page = the 100 most recently updated open MRs instance-wide; paginate if that proves too few.
    let all_ep = "merge_requests?scope=all&state=opened&order_by=updated_at&per_page=100";
    let (mut out, rest) = tokio::try_join!(my_mrs(g, me), get::<Vec<MrSummary>>(g, all_ep, false))?;
    for m in rest {
        if !out.iter().any(|o| (o.project_id, o.iid) == (m.project_id, m.iid)) {
            out.push(m);
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

pub fn merge_roles(assigned: Vec<MrSummary>, reviewing: Vec<MrSummary>) -> Vec<MrSummary> {
    let mut out: Vec<MrSummary> = Vec::with_capacity(assigned.len() + reviewing.len());
    for mut m in assigned {
        m.is_assignee = true;
        if !out.iter().any(|o| (o.project_id, o.iid) == (m.project_id, m.iid)) {
            out.push(m);
        }
    }
    for mut m in reviewing {
        match out.iter_mut().find(|o| (o.project_id, o.iid) == (m.project_id, m.iid)) {
            Some(o) => o.is_reviewer = true,
            None => {
                m.is_reviewer = true;
                out.push(m);
            }
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

pub async fn mr_details(g: &Glab, project_id: u64, iid: u64) -> Result<MrDetails> {
    get(g, &mr_path(project_id, iid), false).await
}

pub async fn mr_commits(g: &Glab, project_id: u64, iid: u64) -> Result<Vec<Commit>> {
    get(g, &format!("{}/commits?per_page=100", mr_path(project_id, iid)), true).await
}

/// Prefer /diffs (GitLab ≥ 15.7); fall back to the deprecated /changes on 404.
pub async fn mr_diffs(g: &Glab, project_id: u64, iid: u64) -> Result<Vec<FileDiff>> {
    let base = mr_path(project_id, iid);
    let raw: Vec<RawDiff> = match get(g, &format!("{base}/diffs?per_page=100"), true).await {
        Err(AppError::NotFound(_)) => {
            #[derive(serde::Deserialize)]
            struct Changes {
                changes: Vec<RawDiff>,
            }
            get::<Changes>(g, &format!("{base}/changes"), false).await?.changes
        }
        r => r?,
    };
    Ok(raw.into_iter().map(diff::from_raw).collect())
}

pub async fn commit_diff(g: &Glab, project_id: u64, sha: &str) -> Result<Vec<FileDiff>> {
    if !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::Rejected(format!("sha {sha}")));
    }
    let raw: Vec<RawDiff> = get(
        g,
        &format!("projects/{project_id}/repository/commits/{sha}/diff?per_page=100"),
        true,
    )
    .await?;
    Ok(raw.into_iter().map(diff::from_raw).collect())
}

pub async fn discussions(g: &Glab, project_id: u64, iid: u64) -> Result<Vec<Discussion>> {
    let mut ds: Vec<Discussion> = get(
        g,
        &format!("{}/discussions?per_page=100", mr_path(project_id, iid)),
        true,
    )
    .await?;
    unmask_names(
        g,
        ds.iter_mut()
            .flat_map(|d| d.notes.iter_mut())
            .map(|n| &mut n.author)
            .collect(),
    )
    .await;
    Ok(ds)
}

/// GitLab returns `name: "****"` for bot users in notes; `users/:id` has the real name.
/// One GET per unknown bot, cached for the app session. Failures keep the masked name.
async fn unmask_names(g: &Glab, mut users: Vec<&mut User>) {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<u64, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let masked = |u: &User| u.id != 0 && !u.name.is_empty() && u.name.chars().all(|c| c == '*');
    let mut todo: Vec<u64> = users.iter().filter(|u| masked(u)).map(|u| u.id).collect();
    todo.sort_unstable();
    todo.dedup();
    for id in todo {
        if cache.lock().unwrap().contains_key(&id) {
            continue;
        }
        if let Ok(u) = get::<User>(g, &format!("users/{id}"), false).await {
            if !u.name.is_empty() {
                cache.lock().unwrap().insert(id, u.name);
            }
        }
    }
    let cache = cache.lock().unwrap();
    for u in users.iter_mut().filter(|u| masked(u)) {
        if let Some(n) = cache.get(&u.id) {
            u.name = n.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_glab() -> Glab {
        Glab {
            bin: concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/fake-glab").into(),
            host: None,
        }
    }

    #[tokio::test]
    async fn full_read_flow_against_fake_glab() {
        let log = std::env::temp_dir().join(format!("fake-glab-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&log);
        std::env::set_var("FAKE_GLAB_LOG", &log);
        let g = fake_glab();

        let me = current_user(&g).await.unwrap();
        assert_eq!(me.username, "me");
        let mrs = my_mrs(&g, &me).await.unwrap();
        // 2 assigned + 2 reviewing, one overlapping → 3 unique.
        assert_eq!(mrs.len(), 3);
        let both = mrs.iter().find(|m| m.iid == 1).unwrap();
        assert!(both.is_assignee && both.is_reviewer);
        assert!(mrs.iter().any(|m| m.iid == 3 && m.is_reviewer && !m.is_assignee));
        // "All" adds others' MRs without role flags and keeps mine flagged, deduped.
        let all = all_mrs(&g, &me).await.unwrap();
        assert_eq!(all.len(), 4);
        assert!(all.iter().any(|m| m.iid == 9 && !m.is_assignee && !m.is_reviewer));
        assert!(all.iter().find(|m| m.iid == 1).unwrap().is_assignee);

        let d = mr_details(&g, 10, 1).await.unwrap();
        assert_eq!(d.diff_refs.unwrap().head_sha, "abc123");
        assert_eq!(mr_commits(&g, 10, 1).await.unwrap().len(), 2);
        let files = mr_diffs(&g, 10, 1).await.unwrap();
        assert_eq!(files.len(), 4);
        assert!(files[0].has_new_line(2));
        assert!(files[2].binary);
        // project 20 has no /diffs fixture → falls back to /changes
        assert_eq!(mr_diffs(&g, 20, 3).await.unwrap().len(), 1);
        let ds = discussions(&g, 10, 1).await.unwrap();
        // Masked bot name ("****") is replaced from users/:id.
        assert!(
            ds.iter().any(|d| d.notes[0].author.name == "sonarqube-bot"),
            "bot name unmasked"
        );
        assert!(ds.iter().any(|d| d.notes[0].system));
        assert!(ds.iter().any(|d| d.notes[0].position.is_some()));

        // Every logged call must be a GET.
        let calls = std::fs::read_to_string(&log).unwrap();
        assert!(calls.lines().count() >= 12);
        for l in calls.lines() {
            assert!(l.starts_with("api --method GET "), "non-GET call: {l}");
        }
    }
}

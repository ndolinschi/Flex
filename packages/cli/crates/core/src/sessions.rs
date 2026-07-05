//! Discovering and resuming sessions the native loop persisted earlier.

use std::path::{Path, PathBuf};

use agentloop_contracts::{AgentEvent, ContentBlock, SessionId, now_ms};
use agentloop_core::SessionStore;
use agentloop_session::JsonlStore;

/// `~/.local/state/flex/sessions` (honoring `XDG_STATE_HOME`) — a
/// sibling of the directory `cli.log` lives in.
pub fn sessions_dir() -> Option<PathBuf> {
    if let Ok(state_home) = std::env::var("XDG_STATE_HOME") {
        if !state_home.trim().is_empty() {
            return Some(PathBuf::from(state_home).join("flex").join("sessions"));
        }
    }
    std::env::var("HOME")
        .ok()
        .filter(|home| !home.trim().is_empty())
        .map(|home| {
            PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("flex")
                .join("sessions")
        })
}

/// `~/.local/state/flex/worktrees` (honoring `XDG_STATE_HOME`) — a sibling of
/// [`sessions_dir`], where isolated session workspaces are provisioned.
pub fn worktrees_dir() -> Option<PathBuf> {
    if let Ok(state_home) = std::env::var("XDG_STATE_HOME") {
        if !state_home.trim().is_empty() {
            return Some(PathBuf::from(state_home).join("flex").join("worktrees"));
        }
    }
    std::env::var("HOME")
        .ok()
        .filter(|home| !home.trim().is_empty())
        .map(|home| {
            PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("flex")
                .join("worktrees")
        })
}

/// One past session, summarized for a picker: id, last-touched time, optional
/// title, and a preview fallback when no title was set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: SessionId,
    pub updated_at_ms: u64,
    pub title: Option<String>,
    pub preview: String,
}

const PREVIEW_MAX_CHARS: usize = 72;

/// Sessions previously run in `cwd`, most recently updated first, capped at
/// `limit`. A missing or unreadable store degrades to an empty list rather
/// than failing session startup over broken history.
pub async fn list_recent_sessions(cwd: &Path, limit: usize) -> Vec<SessionSummary> {
    let Some(root) = sessions_dir() else {
        return Vec::new();
    };
    list_recent_sessions_in(&root, cwd, limit).await
}

/// The most recently updated session in `cwd` — the `--continue` target.
pub async fn most_recent_session(cwd: &Path) -> Option<SessionId> {
    list_recent_sessions(cwd, 1)
        .await
        .into_iter()
        .next()
        .map(|summary| summary.id)
}

/// Whether `id` exists in the persisted store — validates an explicit
/// `--resume <id>` before handing it to the engine, so a stale or mistyped id
/// degrades to "start fresh" instead of a startup failure.
pub async fn session_exists(id: &SessionId) -> bool {
    let Some(root) = sessions_dir() else {
        return false;
    };
    session_exists_in(&root, id).await
}

async fn list_recent_sessions_in(root: &Path, cwd: &Path, limit: usize) -> Vec<SessionSummary> {
    let Ok(store) = JsonlStore::open(root) else {
        return Vec::new();
    };
    let Ok(metas) = store.list().await else {
        return Vec::new();
    };
    let mut summaries = Vec::new();
    for meta in metas.into_iter().filter(|meta| meta.cwd.as_path() == cwd) {
        if summaries.len() >= limit {
            break;
        }
        summaries.push(SessionSummary {
            title: meta.title.clone(),
            preview: preview_for(&store, &meta.id, meta.title.as_deref()).await,
            id: meta.id,
            updated_at_ms: meta.updated_at_ms,
        });
    }
    summaries
}

async fn session_exists_in(root: &Path, id: &SessionId) -> bool {
    match JsonlStore::open(root) {
        Ok(store) => store.get_meta(id).await.is_ok(),
        Err(_) => false,
    }
}

async fn preview_for(store: &JsonlStore, id: &SessionId, title: Option<&str>) -> String {
    if let Some(title) = title.filter(|text| !text.trim().is_empty()) {
        return truncate_preview(title.trim());
    }
    let Ok(events) = store.read(id, 0).await else {
        return "(no messages)".to_owned();
    };
    for (_, event) in events {
        if let AgentEvent::UserMessage { content, .. } = event {
            let text = content.iter().find_map(|block| match block {
                ContentBlock::Markdown { text } => Some(text.clone()),
                _ => None,
            });
            if let Some(text) = text {
                return truncate_preview(text.trim());
            }
        }
    }
    "(no messages)".to_owned()
}

fn truncate_preview(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= PREVIEW_MAX_CHARS {
        collapsed
    } else {
        let head: String = collapsed.chars().take(PREVIEW_MAX_CHARS).collect();
        format!("{head}…")
    }
}

/// Human-readable label for a session row (title when set, else preview).
pub fn session_display_label(summary: &SessionSummary) -> &str {
    summary
        .title
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .unwrap_or(summary.preview.as_str())
}

/// Relative time label such as `2h ago` for picker rows.
pub fn format_relative_time(updated_at_ms: u64) -> String {
    let elapsed_ms = now_ms().saturating_sub(updated_at_ms);
    let secs = elapsed_ms / 1000;
    if secs < 60 {
        "just now".to_owned()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

#[cfg(test)]
mod tests {
    use agentloop_contracts::{MessageId, SessionMeta};
    use tempfile::tempdir;

    use super::*;

    fn meta(id: &str, cwd: &Path, updated_at_ms: u64) -> SessionMeta {
        SessionMeta {
            id: SessionId::from(id),
            title: None,
            agent_id: "native".to_owned(),
            parent_id: None,
            role: None,
            depth: 0,
            provider_session_id: None,
            cwd: cwd.to_path_buf(),
            model: None,
            mode: None,
            isolation: None,
            workspace_id: None,
            base_cwd: None,
            created_at_ms: updated_at_ms,
            updated_at_ms,
        }
    }

    fn user_message(text: &str) -> AgentEvent {
        AgentEvent::UserMessage {
            message_id: MessageId::generate(),
            content: vec![ContentBlock::Markdown {
                text: text.to_owned(),
            }],
        }
    }

    #[test]
    fn truncate_preview_collapses_whitespace_and_caps_length() {
        assert_eq!(truncate_preview("hello   world"), "hello world");
        let long = "word ".repeat(30);
        let truncated = truncate_preview(long.trim());
        assert!(truncated.ends_with('…'));
        assert!(truncated.chars().count() <= PREVIEW_MAX_CHARS + 1);
    }

    #[tokio::test]
    async fn list_recent_sessions_filters_by_cwd_and_sorts_newest_first() {
        let dir = tempdir().expect("tempdir");
        let store = JsonlStore::open(dir.path()).expect("open store");
        let here = PathBuf::from("/project/here");
        let elsewhere = PathBuf::from("/project/elsewhere");

        store.create(meta("older", &here, 100)).await.unwrap();
        store
            .append(
                &SessionId::from("older"),
                &[user_message("first project chat")],
            )
            .await
            .unwrap();
        store.create(meta("newer", &here, 200)).await.unwrap();
        store
            .append(
                &SessionId::from("newer"),
                &[user_message("second project chat")],
            )
            .await
            .unwrap();
        store
            .create(meta("unrelated", &elsewhere, 300))
            .await
            .unwrap();

        let summaries = list_recent_sessions_in(dir.path(), &here, 10).await;
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].id, SessionId::from("newer"));
        assert_eq!(summaries[0].preview, "second project chat");
        assert_eq!(summaries[1].id, SessionId::from("older"));
    }

    #[tokio::test]
    async fn session_summary_prefers_title_over_preview() {
        let dir = tempdir().expect("tempdir");
        let store = JsonlStore::open(dir.path()).expect("open store");
        let cwd = PathBuf::from("/project");
        store.create(meta("s1", &cwd, 1)).await.unwrap();
        store
            .update_meta(
                &SessionId::from("s1"),
                agentloop_contracts::SessionMetaPatch {
                    title: Some("Fix login".to_owned()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        store
            .append(
                &SessionId::from("s1"),
                &[user_message("long fallback preview")],
            )
            .await
            .unwrap();
        let summaries = list_recent_sessions_in(dir.path(), &cwd, 10).await;
        assert_eq!(summaries[0].title.as_deref(), Some("Fix login"));
        assert_eq!(summaries[0].preview, "Fix login");
        assert_eq!(session_display_label(&summaries[0]), "Fix login");
    }

    #[tokio::test]
    async fn list_recent_sessions_respects_limit() {
        let dir = tempdir().expect("tempdir");
        let store = JsonlStore::open(dir.path()).expect("open store");
        let cwd = PathBuf::from("/project");
        for i in 0..5 {
            store
                .create(meta(&format!("s{i}"), &cwd, i as u64))
                .await
                .unwrap();
        }
        assert_eq!(list_recent_sessions_in(dir.path(), &cwd, 2).await.len(), 2);
    }

    #[tokio::test]
    async fn session_with_no_user_message_previews_as_no_messages() {
        let dir = tempdir().expect("tempdir");
        let store = JsonlStore::open(dir.path()).expect("open store");
        let cwd = PathBuf::from("/project");
        store.create(meta("empty", &cwd, 1)).await.unwrap();

        let summaries = list_recent_sessions_in(dir.path(), &cwd, 10).await;
        assert_eq!(summaries[0].preview, "(no messages)");
    }

    #[tokio::test]
    async fn session_exists_reflects_the_store() {
        let dir = tempdir().expect("tempdir");
        let store = JsonlStore::open(dir.path()).expect("open store");
        let cwd = PathBuf::from("/project");
        store.create(meta("present", &cwd, 1)).await.unwrap();

        assert!(session_exists_in(dir.path(), &SessionId::from("present")).await);
        assert!(!session_exists_in(dir.path(), &SessionId::from("absent")).await);
    }

    #[tokio::test]
    async fn missing_store_root_yields_empty_list_not_a_panic() {
        let dir = tempdir().expect("tempdir");
        let missing = dir.path().join("does-not-exist-yet");
        // JsonlStore::open creates the directory on demand, so this exercises
        // the "first run, nothing persisted yet" path rather than a real I/O
        // failure — the empty-list contract holds either way.
        let summaries = list_recent_sessions_in(&missing, Path::new("/project"), 10).await;
        assert!(summaries.is_empty());
    }
}

//! Append-only JSONL [`SessionStore`] implementation.
//!
//! Each session owns one `.jsonl` file. Lines are either metadata snapshots,
//! events, or delete tombstones; reopening the store replays those records into
//! the same in-memory shape as [`MemoryStore`](crate::MemoryStore).

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use agentloop_contracts::{
    AgentEvent, CompactionSummary, SessionId, SessionMeta, SessionMetaPatch, now_ms,
};
use agentloop_core::{SessionStore, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
enum LineRecord {
    Meta { meta: SessionMeta },
    Event { event: AgentEvent },
    Delete,
}

#[derive(Debug, Clone)]
struct Record {
    meta: SessionMeta,
    events: Vec<AgentEvent>,
}

/// JSONL-backed append-only session store.
#[derive(Debug)]
pub struct JsonlStore {
    root: PathBuf,
    sessions: Mutex<HashMap<SessionId, Record>>,
}

impl JsonlStore {
    /// Open or create a JSONL store rooted at `root`.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(io_error)?;
        let sessions = load_sessions(&root)?;
        Ok(Self {
            root,
            sessions: Mutex::new(sessions),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<SessionId, Record>> {
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn path_for(&self, id: &SessionId) -> PathBuf {
        self.root
            .join(format!("{}.jsonl", file_component(id.as_str())))
    }

    fn append_record(&self, id: &SessionId, record: &LineRecord) -> Result<(), StoreError> {
        append_record(&self.path_for(id), record)
    }
}

#[async_trait]
impl SessionStore for JsonlStore {
    async fn create(&self, meta: SessionMeta) -> Result<(), StoreError> {
        let mut sessions = self.lock();
        if sessions.contains_key(&meta.id) {
            return Err(StoreError::SessionExists(meta.id));
        }
        self.append_record(&meta.id, &LineRecord::Meta { meta: meta.clone() })?;
        sessions.insert(
            meta.id.clone(),
            Record {
                meta,
                events: Vec::new(),
            },
        );
        Ok(())
    }

    async fn append(&self, id: &SessionId, events: &[AgentEvent]) -> Result<u64, StoreError> {
        let mut sessions = self.lock();
        let record = sessions
            .get_mut(id)
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))?;
        let first_seq = record.events.len() as u64;
        for event in events {
            self.append_record(
                id,
                &LineRecord::Event {
                    event: event.clone(),
                },
            )?;
        }
        if !events.is_empty() {
            record.events.extend_from_slice(events);
            record.meta.updated_at_ms = now_ms();
            self.append_record(
                id,
                &LineRecord::Meta {
                    meta: record.meta.clone(),
                },
            )?;
        }
        Ok(first_seq)
    }

    async fn read(
        &self,
        id: &SessionId,
        from_seq: u64,
    ) -> Result<Vec<(u64, AgentEvent)>, StoreError> {
        let sessions = self.lock();
        let record = sessions
            .get(id)
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))?;
        Ok(record
            .events
            .iter()
            .enumerate()
            .map(|(i, event)| (i as u64, event.clone()))
            .filter(|(seq, _)| *seq >= from_seq)
            .collect())
    }

    async fn list(&self) -> Result<Vec<SessionMeta>, StoreError> {
        let sessions = self.lock();
        let mut metas = sessions
            .values()
            .map(|record| record.meta.clone())
            .collect::<Vec<_>>();
        metas.sort_by_key(|meta| std::cmp::Reverse(meta.updated_at_ms));
        Ok(metas)
    }

    async fn get_meta(&self, id: &SessionId) -> Result<SessionMeta, StoreError> {
        let sessions = self.lock();
        sessions
            .get(id)
            .map(|record| record.meta.clone())
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))
    }

    async fn update_meta(&self, id: &SessionId, patch: SessionMetaPatch) -> Result<(), StoreError> {
        let mut sessions = self.lock();
        let record = sessions
            .get_mut(id)
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))?;
        let mut next = record.meta.clone();
        let SessionMetaPatch {
            title,
            provider_session_id,
            model,
            mode,
        } = patch;
        if let Some(title) = title {
            next.title = Some(title);
        }
        if let Some(provider_session_id) = provider_session_id {
            next.provider_session_id = Some(provider_session_id);
        }
        if let Some(model) = model {
            next.model = Some(model);
        }
        if let Some(mode) = mode {
            next.mode = Some(mode);
        }
        next.updated_at_ms = now_ms();
        self.append_record(&next.id, &LineRecord::Meta { meta: next.clone() })?;
        record.meta = next;
        Ok(())
    }

    async fn delete(&self, id: &SessionId) -> Result<(), StoreError> {
        let mut sessions = self.lock();
        if !sessions.contains_key(id) {
            return Err(StoreError::SessionNotFound(id.clone()));
        }
        self.append_record(id, &LineRecord::Delete)?;
        sessions.remove(id);
        Ok(())
    }

    async fn record_compaction(
        &self,
        id: &SessionId,
        compaction: CompactionSummary,
    ) -> Result<(), StoreError> {
        self.append(
            id,
            &[AgentEvent::CompactionBoundary {
                summary: compaction,
            }],
        )
        .await
        .map(|_| ())
    }
}

fn load_sessions(root: &Path) -> Result<HashMap<SessionId, Record>, StoreError> {
    let mut sessions = HashMap::new();
    for entry in fs::read_dir(root).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") || !path.is_file() {
            continue;
        }
        if let Some((id, record)) = load_file(&path)? {
            sessions.insert(id, record);
        }
    }
    Ok(sessions)
}

fn load_file(path: &Path) -> Result<Option<(SessionId, Record)>, StoreError> {
    let file = File::open(path).map_err(io_error)?;
    let reader = BufReader::new(file);
    let mut current: Option<Record> = None;

    for (line_number, line) in reader.lines().enumerate() {
        let line = line.map_err(io_error)?;
        if line.trim().is_empty() {
            continue;
        }
        let record: LineRecord = serde_json::from_str(&line).map_err(|source| {
            StoreError::Corrupt(format!(
                "{}:{} is not a valid JSONL store record: {source}",
                path.display(),
                line_number + 1
            ))
        })?;
        match record {
            LineRecord::Meta { meta } => match &mut current {
                Some(record) => record.meta = meta,
                None => {
                    current = Some(Record {
                        meta,
                        events: Vec::new(),
                    });
                }
            },
            LineRecord::Event { event } => match &mut current {
                Some(record) => record.events.push(event),
                None => {
                    return Err(StoreError::Corrupt(format!(
                        "{}:{} contains an event before session metadata",
                        path.display(),
                        line_number + 1
                    )));
                }
            },
            LineRecord::Delete => current = None,
        }
    }

    Ok(current.map(|record| (record.meta.id.clone(), record)))
}

fn append_record(path: &Path, record: &LineRecord) -> Result<(), StoreError> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(io_error)?;
    serde_json::to_writer(&mut file, record)
        .map_err(|source| StoreError::Io(source.to_string()))?;
    writeln!(file).map_err(io_error)
}

fn io_error(source: std::io::Error) -> StoreError {
    StoreError::Io(source.to_string())
}

fn file_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' => out.push(*byte as char),
            byte => {
                out.push('%');
                out.push(hex_digit(byte >> 4));
                out.push(hex_digit(byte & 0x0f));
            }
        }
    }
    out
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + (value - 10)) as char,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use agentloop_contracts::{ModelRef, TurnId};
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    use super::*;

    fn meta(id: &str) -> SessionMeta {
        SessionMeta {
            id: SessionId::from(id),
            title: None,
            agent_id: "native".to_owned(),
            parent_id: None,
            provider_session_id: None,
            cwd: PathBuf::from("/workspace"),
            model: None,
            mode: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    fn event(turn: &str) -> AgentEvent {
        AgentEvent::TurnStarted {
            turn_id: TurnId::from(turn),
        }
    }

    #[tokio::test]
    async fn persists_events_across_reopen() {
        let dir = tempdir().unwrap();
        let id = SessionId::from("s1");
        {
            let store = JsonlStore::open(dir.path()).unwrap();
            store.create(meta("s1")).await.unwrap();
            assert_eq!(
                store
                    .append(&id, &[event("t0"), event("t1")])
                    .await
                    .unwrap(),
                0
            );
            assert_eq!(store.append(&id, &[event("t2")]).await.unwrap(), 2);
        }

        let reopened = JsonlStore::open(dir.path()).unwrap();
        let events = reopened.read(&id, 1).await.unwrap();
        assert_eq!(events, vec![(1, event("t1")), (2, event("t2"))]);
        assert_eq!(reopened.append(&id, &[]).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn meta_updates_survive_reopen() {
        let dir = tempdir().unwrap();
        let id = SessionId::from("s1");
        {
            let store = JsonlStore::open(dir.path()).unwrap();
            store.create(meta("s1")).await.unwrap();
            store
                .update_meta(
                    &id,
                    SessionMetaPatch {
                        title: Some("updated".to_owned()),
                        provider_session_id: Some("remote".to_owned()),
                        model: Some(ModelRef::from("anthropic/model-x")),
                        mode: None,
                    },
                )
                .await
                .unwrap();
        }

        let reopened = JsonlStore::open(dir.path()).unwrap();
        let meta = reopened.get_meta(&id).await.unwrap();
        assert_eq!(meta.title.as_deref(), Some("updated"));
        assert_eq!(meta.provider_session_id.as_deref(), Some("remote"));
        assert_eq!(meta.model, Some(ModelRef::from("anthropic/model-x")));
    }

    #[tokio::test]
    async fn delete_tombstone_survives_reopen() {
        let dir = tempdir().unwrap();
        let id = SessionId::from("s1");
        {
            let store = JsonlStore::open(dir.path()).unwrap();
            store.create(meta("s1")).await.unwrap();
            store.append(&id, &[event("t0")]).await.unwrap();
            store.delete(&id).await.unwrap();
        }

        let reopened = JsonlStore::open(dir.path()).unwrap();
        assert!(reopened.list().await.unwrap().is_empty());
        assert!(matches!(
            reopened.get_meta(&id).await.unwrap_err(),
            StoreError::SessionNotFound(_)
        ));
    }
}

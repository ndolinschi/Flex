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
    AgentEvent, CheckpointRef, CompactionSummary, SessionId, SessionMeta, SessionMetaPatch, now_ms,
};
use agentloop_core::{SessionStore, StoreError, StoredEvent};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
enum LineRecord {
    Meta {
        meta: SessionMeta,
    },
    Event {
        /// Wall-clock time the event was appended. Stamped once at emit and
        /// persisted so replay never rewrites it to "now". `#[serde(default)]`
        /// so pre-existing logs written before this field deserialize to 0;
        /// such legacy lines get a reconstructed, monotonic ts at load time.
        #[serde(default)]
        ts_ms: u64,
        event: AgentEvent,
    },
    Delete,
    /// Internal to this file format — not a wire type. A named pointer at a
    /// `seq` the log already contains.
    Checkpoint {
        checkpoint: CheckpointRef,
    },
}

#[derive(Debug, Clone)]
struct Record {
    meta: SessionMeta,
    /// Appended events paired with the wall-clock `ts_ms` captured at emit.
    /// Index is the per-session sequence number (gapless, starts at 0).
    events: Vec<(u64, AgentEvent)>,
    checkpoints: Vec<CheckpointRef>,
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
                checkpoints: Vec::new(),
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
        // Stamp emit time once per appended event. append runs synchronously at
        // emit, so this is the true emit time; it is persisted and reused on
        // replay so historical timestamps never collapse to "now".
        let ts_ms = now_ms();
        for event in events {
            self.append_record(
                id,
                &LineRecord::Event {
                    ts_ms,
                    event: event.clone(),
                },
            )?;
        }
        if !events.is_empty() {
            record
                .events
                .extend(events.iter().map(|event| (ts_ms, event.clone())));
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

    async fn read(&self, id: &SessionId, from_seq: u64) -> Result<Vec<StoredEvent>, StoreError> {
        let sessions = self.lock();
        let record = sessions
            .get(id)
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))?;
        Ok(record
            .events
            .iter()
            .enumerate()
            .filter(|(seq, _)| *seq as u64 >= from_seq)
            .map(|(seq, (ts_ms, event))| StoredEvent {
                seq: seq as u64,
                ts_ms: *ts_ms,
                event: event.clone(),
            })
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
            cwd,
            workspace_id,
            base_cwd,
            reuse_workspace_id,
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
        if let Some(cwd) = cwd {
            next.cwd = cwd;
        }
        if let Some(mode) = mode {
            next.mode = Some(mode);
        }
        // Empty string / empty path clears; a non-empty value sets. Matches
        // the convention in `MemoryStore::update_meta` — see there.
        if let Some(workspace_id) = workspace_id {
            next.workspace_id = if workspace_id.is_empty() {
                None
            } else {
                Some(workspace_id)
            };
        }
        if let Some(base_cwd) = base_cwd {
            next.base_cwd = if base_cwd.as_os_str().is_empty() {
                None
            } else {
                Some(base_cwd)
            };
        }
        if let Some(reuse_workspace_id) = reuse_workspace_id {
            next.reuse_workspace_id = if reuse_workspace_id.is_empty() {
                None
            } else {
                Some(reuse_workspace_id)
            };
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

    async fn record_checkpoint(
        &self,
        id: &SessionId,
        checkpoint: CheckpointRef,
    ) -> Result<(), StoreError> {
        let mut sessions = self.lock();
        let record = sessions
            .get_mut(id)
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))?;
        self.append_record(
            id,
            &LineRecord::Checkpoint {
                checkpoint: checkpoint.clone(),
            },
        )?;
        record.checkpoints.push(checkpoint);
        Ok(())
    }

    async fn list_checkpoints(&self, id: &SessionId) -> Result<Vec<CheckpointRef>, StoreError> {
        let sessions = self.lock();
        let record = sessions
            .get(id)
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))?;
        Ok(record.checkpoints.clone())
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
                        checkpoints: Vec::new(),
                    });
                }
            },
            LineRecord::Event { ts_ms, event } => match &mut current {
                Some(record) => record.events.push((ts_ms, event)),
                None => {
                    return Err(StoreError::Corrupt(format!(
                        "{}:{} contains an event before session metadata",
                        path.display(),
                        line_number + 1
                    )));
                }
            },
            LineRecord::Checkpoint { checkpoint } => match &mut current {
                Some(record) => record.checkpoints.push(checkpoint),
                None => {
                    return Err(StoreError::Corrupt(format!(
                        "{}:{} contains a checkpoint before session metadata",
                        path.display(),
                        line_number + 1
                    )));
                }
            },
            LineRecord::Delete => current = None,
        }
    }

    if let Some(record) = &mut current {
        reconstruct_legacy_ts(record);
    }

    Ok(current.map(|record| (record.meta.id.clone(), record)))
}

/// Backfill a stable, monotonic `ts_ms` onto legacy event lines that predate
/// the persisted timestamp (`ts_ms == 0`). Old sessions must render stable,
/// non-collapsing, roughly-ordered times that DON'T jump to "now" on reload,
/// so we reconstruct deterministically: carry forward the max of the previous
/// event's effective ts, any checkpoint ts at/before this seq, and
/// `meta.created_at_ms` as the floor. Events that already carry a real ts are
/// left untouched (but still raise the running floor for later legacy lines).
fn reconstruct_legacy_ts(record: &mut Record) {
    let mut floor = record.meta.created_at_ms;
    for (seq, (ts_ms, _)) in record.events.iter_mut().enumerate() {
        // Any checkpoint recorded at or before this seq is a known real time.
        for checkpoint in &record.checkpoints {
            if checkpoint.seq <= seq as u64 {
                floor = floor.max(checkpoint.ts_ms);
            }
        }
        if *ts_ms == 0 {
            *ts_ms = floor;
        } else {
            floor = floor.max(*ts_ms);
        }
    }
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
            role: None,
            depth: 0,
            provider_session_id: None,
            cwd: PathBuf::from("/workspace"),
            model: None,
            fallback_models: Vec::new(),
            mode: None,
            isolation: None,
            workspace_id: None,
            executor: None,
            base_cwd: None,
            reuse_workspace_id: None,
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
        let seqs: Vec<(u64, AgentEvent)> = events
            .iter()
            .map(|stored| (stored.seq, stored.event.clone()))
            .collect();
        assert_eq!(seqs, vec![(1, event("t1")), (2, event("t2"))]);
        // The persisted ts round-trips as a real (non-zero) wall-clock time,
        // stable across the reopen above.
        assert!(events.iter().all(|stored| stored.ts_ms > 0));
        assert_eq!(reopened.append(&id, &[]).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn append_stamps_and_persists_real_ts_across_reopen() {
        let dir = tempdir().unwrap();
        let id = SessionId::from("s1");
        let (t0_ts, t1_ts);
        {
            let store = JsonlStore::open(dir.path()).unwrap();
            store.create(meta("s1")).await.unwrap();
            store.append(&id, &[event("t0")]).await.unwrap();
            // Force a distinct wall-clock reading for the second append.
            std::thread::sleep(std::time::Duration::from_millis(2));
            store.append(&id, &[event("t1")]).await.unwrap();
            let live = store.read(&id, 0).await.unwrap();
            t0_ts = live[0].ts_ms;
            t1_ts = live[1].ts_ms;
            assert!(t0_ts > 0 && t1_ts > 0, "append stamps a real ts");
            assert!(t1_ts >= t0_ts, "ts is non-decreasing across appends");
            assert!(t1_ts > t0_ts, "distinct appends keep distinct ts");
        }

        // Reopening must not rewrite the stored timestamps to "now".
        let reopened = JsonlStore::open(dir.path()).unwrap();
        let events = reopened.read(&id, 0).await.unwrap();
        assert_eq!(events[0].ts_ms, t0_ts);
        assert_eq!(events[1].ts_ms, t1_ts);
    }

    #[tokio::test]
    async fn legacy_event_lines_without_ts_reconstruct_stable_time() {
        // Simulate an on-disk log written before ts_ms existed: Event lines
        // carry no ts field, so they deserialize to 0 and must be backfilled.
        let dir = tempdir().unwrap();
        let id = SessionId::from("legacy");
        let path = dir.path().join("legacy.jsonl");
        {
            let mut m = meta("legacy");
            m.created_at_ms = 5_000;
            append_record(&path, &LineRecord::Meta { meta: m }).unwrap();
            // Hand-write legacy event lines with NO ts_ms key.
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(
                file,
                "{}",
                serde_json::json!({
                    "record": "event",
                    "event": { "kind": "turn_started", "turn_id": "t0" }
                })
            )
            .unwrap();
            writeln!(
                file,
                "{}",
                serde_json::json!({
                    "record": "event",
                    "event": { "kind": "turn_started", "turn_id": "t1" }
                })
            )
            .unwrap();
        }

        let store = JsonlStore::open(dir.path()).unwrap();
        let events = store.read(&id, 0).await.unwrap();
        assert_eq!(events.len(), 2);
        // Reconstructed times are non-zero, floored at created_at_ms, stable,
        // and non-decreasing — never "now".
        assert_eq!(events[0].ts_ms, 5_000);
        assert_eq!(events[1].ts_ms, 5_000);
        assert!(events[1].ts_ms >= events[0].ts_ms);
        assert!(events[0].ts_ms < now_ms(), "legacy ts must not be 'now'");
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
                        ..Default::default()
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
    async fn satisfies_the_session_store_conformance_suite() {
        let dir = tempdir().unwrap();
        agentloop_testkit::assert_store_conformance(JsonlStore::open(dir.path()).unwrap())
            .await
            .unwrap();
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

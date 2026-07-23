use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::{
    AgentEvent, CheckpointLabel, CheckpointRef, SessionId, SessionMeta, TurnId, now_ms,
};
use agentloop_core::{SessionStore, StoreError};

fn meta(id: &str) -> SessionMeta {
    SessionMeta {
        id: SessionId::from(id),
        title: None,
        agent_id: "conformance".to_owned(),
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

fn turn_event(turn: &str) -> AgentEvent {
    AgentEvent::TurnStarted {
        turn_id: TurnId::from(turn),
    }
}

pub async fn assert_store_conformance<S>(store: S) -> Result<(), StoreError>
where
    S: SessionStore + 'static,
{
    let store: Arc<dyn SessionStore> = Arc::new(store);
    assert_gapless_seq_on_append(&store).await?;
    assert_append_then_immediately_readable(&store).await?;
    assert_append_roundtrips_monotonic_ts(&store).await?;
    assert_checkpoint_roundtrip(&store).await?;
    assert_concurrent_appends_preserve_order_and_no_loss(store).await?;
    Ok(())
}

async fn assert_gapless_seq_on_append(store: &Arc<dyn SessionStore>) -> Result<(), StoreError> {
    let id = SessionId::from("conformance-gapless");
    store.create(meta(id.as_str())).await?;
    let first = store
        .append(&id, &[turn_event("t0"), turn_event("t1")])
        .await?;
    assert_eq!(first, 0, "first batch starts at seq 0");
    let second = store.append(&id, &[turn_event("t2")]).await?;
    assert_eq!(
        second, 2,
        "second batch continues from the prior batch's end"
    );
    let seqs: Vec<u64> = store
        .read(&id, 0)
        .await?
        .into_iter()
        .map(|stored| stored.seq)
        .collect();
    assert_eq!(seqs, vec![0, 1, 2], "seqs are gapless and start at 0");
    Ok(())
}

async fn assert_append_roundtrips_monotonic_ts(
    store: &Arc<dyn SessionStore>,
) -> Result<(), StoreError> {
    let id = SessionId::from("conformance-ts");
    store.create(meta(id.as_str())).await?;
    store.append(&id, &[turn_event("t0")]).await?;
    std::thread::sleep(std::time::Duration::from_millis(2));
    store.append(&id, &[turn_event("t1")]).await?;

    let events = store.read(&id, 0).await?;
    assert_eq!(events.len(), 2);
    assert!(
        events[0].ts_ms > 0,
        "append stamps a real ts_ms, not a placeholder"
    );
    assert!(
        events[1].ts_ms > events[0].ts_ms,
        "distinct appends keep distinct, non-decreasing ts_ms"
    );
    Ok(())
}

async fn assert_append_then_immediately_readable(
    store: &Arc<dyn SessionStore>,
) -> Result<(), StoreError> {
    let id = SessionId::from("conformance-readable");
    store.create(meta(id.as_str())).await?;
    store.append(&id, &[turn_event("t0")]).await?;
    let events = store.read(&id, 0).await?;
    assert_eq!(
        events.len(),
        1,
        "an appended event is visible to a read that follows it"
    );
    Ok(())
}

async fn assert_checkpoint_roundtrip(store: &Arc<dyn SessionStore>) -> Result<(), StoreError> {
    let id = SessionId::from("conformance-checkpoint");
    store.create(meta(id.as_str())).await?;
    store.append(&id, &[turn_event("t0")]).await?;
    let checkpoint = CheckpointRef {
        session_id: id.clone(),
        seq: 0,
        turn_id: Some(TurnId::from("t0")),
        ts_ms: now_ms(),
        label: CheckpointLabel::TurnCompleted,
    };
    store.record_checkpoint(&id, checkpoint.clone()).await?;
    let checkpoints = store.list_checkpoints(&id).await?;
    assert_eq!(
        checkpoints,
        vec![checkpoint],
        "a recorded checkpoint round-trips through list_checkpoints"
    );
    Ok(())
}

async fn assert_concurrent_appends_preserve_order_and_no_loss(
    store: Arc<dyn SessionStore>,
) -> Result<(), StoreError> {
    let id = SessionId::from("conformance-concurrent");
    store.create(meta(id.as_str())).await?;

    const WRITERS: usize = 8;
    let mut handles = Vec::with_capacity(WRITERS);
    for writer in 0..WRITERS {
        let store = store.clone();
        let id = id.clone();
        handles.push(tokio::spawn(async move {
            store
                .append(&id, &[turn_event(&format!("writer-{writer}"))])
                .await
        }));
    }
    let mut seqs = Vec::with_capacity(WRITERS);
    for handle in handles {
        match handle.await {
            Ok(result) => seqs.push(result?),
            Err(err) => panic!("writer task join failed: {err}"),
        }
    }
    seqs.sort_unstable();
    let expected: Vec<u64> = (0..WRITERS as u64).collect();
    assert_eq!(
        seqs, expected,
        "concurrent appends assign every seq exactly once, with no gaps or duplicates"
    );

    let logged = store.read(&id, 0).await?;
    assert_eq!(
        logged.len(),
        WRITERS,
        "every concurrently appended event lands in the log exactly once"
    );
    Ok(())
}

//! Root-session workspace isolation: policy resolution, deferred provisioning
//! on first prompt, reuse of an existing worktree, and resume repointing.

use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::{AgentEvent, IsolationPolicy, NewSessionParams, SessionMeta};
use agentloop_core::{Agent, SessionStore, StoredEvent, Workspace as CoreWorkspace, Workspaces};
use agentloop_loop::NativeAgentBuilder;
use agentloop_loop::roles::RoleSpec;
use agentloop_session::MemoryStore;
use agentloop_testkit::MockWorkspaces;

fn build_agent(
    store: Arc<MemoryStore>,
    workspace: Option<Arc<dyn Workspaces>>,
    roles: Vec<RoleSpec>,
) -> Arc<agentloop_loop::NativeAgent> {
    let mut builder = NativeAgentBuilder::new(store)
        .system_prompt("test agent")
        .roles(roles);
    if let Some(workspace) = workspace {
        builder = builder.workspace(workspace);
    }
    builder.build()
}

fn params(base: &str, isolation: Option<IsolationPolicy>) -> NewSessionParams {
    NewSessionParams {
        cwd: Some(PathBuf::from(base)),
        isolation,
        ..NewSessionParams::default()
    }
}

async fn provisioned_events(store: &MemoryStore, id: &agentloop_contracts::SessionId) -> usize {
    store
        .read(id, 0)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|StoredEvent { event: e, .. }| matches!(e, AgentEvent::WorkspaceProvisioned { .. }))
        .count()
}

#[allow(clippy::expect_used)]
async fn get_meta(store: &MemoryStore, id: &agentloop_contracts::SessionId) -> SessionMeta {
    store.get_meta(id).await.expect("meta")
}

#[tokio::test]
async fn required_isolation_defers_provisioning_until_first_prompt() {
    let store = Arc::new(MemoryStore::new());
    let mock = Arc::new(MockWorkspaces::new());
    let agent = build_agent(store.clone(), Some(mock.clone()), Vec::new());

    let id = agent
        .create_session(params("/repo", Some(IsolationPolicy::Required)))
        .await
        .expect("required isolation succeeds at create time when a backend is configured");

    // At create time nothing is provisioned yet: cwd still points at the
    // project, workspace_id is empty, no WorkspaceProvisioned event yet.
    let meta = get_meta(&store, &id).await;
    assert_eq!(meta.cwd, PathBuf::from("/repo"));
    assert_eq!(meta.workspace_id, None);
    assert_eq!(meta.base_cwd, None);
    assert_eq!(meta.isolation, Some(IsolationPolicy::Required));
    assert_eq!(mock.provision_calls(), 0, "provisioning is deferred");
    assert_eq!(provisioned_events(&store, &id).await, 0);

    // Simulate the first prompt's ensure step.
    agent
        .ensure_workspace_for_test(&id)
        .await
        .expect("first-turn provisioning succeeds");

    let meta = get_meta(&store, &id).await;
    assert_eq!(
        meta.cwd,
        PathBuf::from("/tmp/mock-workspaces").join(id.to_string())
    );
    assert_eq!(meta.base_cwd, Some(PathBuf::from("/repo")));
    assert_eq!(meta.workspace_id, Some(format!("ws-{id}")));
    assert_eq!(meta.reuse_workspace_id, None);
    assert_eq!(mock.provision_calls(), 1);
    assert_eq!(provisioned_events(&store, &id).await, 1);
}

#[tokio::test]
async fn ensure_is_idempotent_after_first_provision() {
    let store = Arc::new(MemoryStore::new());
    let mock = Arc::new(MockWorkspaces::new());
    let agent = build_agent(store.clone(), Some(mock.clone()), Vec::new());

    let id = agent
        .create_session(params("/repo", Some(IsolationPolicy::Required)))
        .await
        .expect("session");
    agent.ensure_workspace_for_test(&id).await.expect("first");
    agent.ensure_workspace_for_test(&id).await.expect("second");

    assert_eq!(
        mock.provision_calls(),
        1,
        "workspace_id gates further provisioning after the first turn"
    );
    assert_eq!(provisioned_events(&store, &id).await, 1);
}

#[tokio::test]
async fn optional_isolation_falls_back_when_backend_declines() {
    let store = Arc::new(MemoryStore::new());
    let mock = Arc::new(MockWorkspaces::unavailable());
    let agent = build_agent(store.clone(), Some(mock), Vec::new());

    let id = agent
        .create_session(params("/repo", Some(IsolationPolicy::Optional)))
        .await
        .expect("optional isolation never fails at create");
    agent
        .ensure_workspace_for_test(&id)
        .await
        .expect("optional ensure never fails");

    let meta = get_meta(&store, &id).await;
    assert_eq!(meta.cwd, PathBuf::from("/repo"));
    assert_eq!(meta.workspace_id, None);
    assert_eq!(meta.base_cwd, None);
    assert_eq!(meta.isolation, Some(IsolationPolicy::Optional));
    assert_eq!(provisioned_events(&store, &id).await, 0);
}

#[tokio::test]
async fn required_isolation_fails_at_first_turn_when_backend_cannot_provision() {
    let store = Arc::new(MemoryStore::new());
    let mock = Arc::new(MockWorkspaces::unavailable());
    let agent = build_agent(store.clone(), Some(mock), Vec::new());

    // Create still succeeds — we only find out on the first turn.
    let id = agent
        .create_session(params("/repo", Some(IsolationPolicy::Required)))
        .await
        .expect("create defers the provision failure");
    let result = agent.ensure_workspace_for_test(&id).await;
    assert!(
        result.is_err(),
        "required isolation must fail on first turn when unprovisionable"
    );
}

#[tokio::test]
async fn required_isolation_fails_without_a_backend() {
    let store = Arc::new(MemoryStore::new());
    let agent = build_agent(store.clone(), None, Vec::new());

    let result = agent
        .create_session(params("/repo", Some(IsolationPolicy::Required)))
        .await;
    assert!(
        result.is_err(),
        "required isolation without a backend still fails fast at create"
    );
}

#[tokio::test]
async fn no_policy_means_no_isolation() {
    let store = Arc::new(MemoryStore::new());
    let mock = Arc::new(MockWorkspaces::new());
    let agent = build_agent(store.clone(), Some(mock.clone()), Vec::new());

    let id = agent
        .create_session(params("/repo", None))
        .await
        .expect("session");
    agent
        .ensure_workspace_for_test(&id)
        .await
        .expect("ensure is a no-op without a policy");

    let meta = get_meta(&store, &id).await;
    assert_eq!(meta.cwd, PathBuf::from("/repo"));
    assert_eq!(meta.isolation, None);
    assert_eq!(
        mock.provision_calls(),
        0,
        "backend untouched when policy is Never"
    );
}

#[tokio::test]
async fn role_declared_isolation_drives_the_root_session() {
    let store = Arc::new(MemoryStore::new());
    let mock = Arc::new(MockWorkspaces::new());
    let main = RoleSpec {
        isolation: IsolationPolicy::Required,
        ..RoleSpec::new("main")
    };
    let agent = build_agent(store.clone(), Some(mock.clone()), vec![main]);

    let id = agent
        .create_session(params("/repo", None))
        .await
        .expect("role-required isolation still creates the session eagerly");
    agent
        .ensure_workspace_for_test(&id)
        .await
        .expect("first-turn provision");

    let meta = get_meta(&store, &id).await;
    assert_eq!(meta.workspace_id, Some(format!("ws-{id}")));
    assert_eq!(meta.isolation, Some(IsolationPolicy::Required));
    assert_eq!(mock.provision_calls(), 1);
}

#[tokio::test]
async fn reuse_workspace_id_attaches_existing_worktree() {
    let store = Arc::new(MemoryStore::new());
    let seeded = CoreWorkspace {
        id: "ws-existing".to_owned(),
        root: PathBuf::from("/tmp/mock-workspaces/existing"),
        base_ref: "mockbase".to_owned(),
    };
    let mock = Arc::new(MockWorkspaces::new().seed_workspace("/repo", seeded.clone()));
    let agent = build_agent(store.clone(), Some(mock.clone()), Vec::new());

    let id = agent
        .create_session(NewSessionParams {
            cwd: Some(PathBuf::from("/repo")),
            isolation: Some(IsolationPolicy::Required),
            reuse_workspace_id: Some("ws-existing".to_owned()),
            ..NewSessionParams::default()
        })
        .await
        .expect("session");

    // Reuse hint is recorded on meta and no worktree is created yet.
    assert_eq!(
        get_meta(&store, &id).await.reuse_workspace_id.as_deref(),
        Some("ws-existing")
    );
    assert_eq!(mock.provision_calls(), 0);
    assert_eq!(mock.attach_calls(), 0);

    agent
        .ensure_workspace_for_test(&id)
        .await
        .expect("attach on first turn");

    let meta = get_meta(&store, &id).await;
    assert_eq!(meta.cwd, seeded.root);
    assert_eq!(meta.workspace_id.as_deref(), Some("ws-existing"));
    assert_eq!(meta.base_cwd, Some(PathBuf::from("/repo")));
    assert_eq!(
        meta.reuse_workspace_id, None,
        "reuse hint cleared after use"
    );
    assert_eq!(mock.attach_calls(), 1);
    assert_eq!(
        mock.provision_calls(),
        0,
        "reuse must not spawn a fresh worktree"
    );
    assert_eq!(provisioned_events(&store, &id).await, 1);
}

#[tokio::test]
async fn resume_repoints_cwd_when_the_workspace_is_gone() {
    let store = Arc::new(MemoryStore::new());
    let mock = Arc::new(MockWorkspaces::new());
    let agent = build_agent(store.clone(), Some(mock), Vec::new());

    let id = agent
        .create_session(params("/repo", Some(IsolationPolicy::Required)))
        .await
        .expect("session");
    agent
        .ensure_workspace_for_test(&id)
        .await
        .expect("provision");
    assert_ne!(get_meta(&store, &id).await.cwd, PathBuf::from("/repo"));

    agent.resume_session(&id).await.expect("resume");

    let meta = get_meta(&store, &id).await;
    assert_eq!(
        meta.cwd,
        PathBuf::from("/repo"),
        "cwd repointed to the base on resume"
    );
}

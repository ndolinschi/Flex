//! Root-session workspace isolation: policy resolution, cwd redirection,
//! provisioning events, graceful fallback, and resume repointing.

use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::{AgentEvent, IsolationPolicy, NewSessionParams};
use agentloop_core::{Agent, SessionStore, Workspaces};
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
        .filter(|(_, e)| matches!(e, AgentEvent::WorkspaceProvisioned { .. }))
        .count()
}

#[tokio::test]
async fn required_isolation_redirects_cwd_and_emits_event() {
    let store = Arc::new(MemoryStore::new());
    let mock = Arc::new(MockWorkspaces::new());
    let agent = build_agent(store.clone(), Some(mock.clone()), Vec::new());

    let id = agent
        .create_session(params("/repo", Some(IsolationPolicy::Required)))
        .await
        .expect("required isolation succeeds when the backend can provision");

    let meta = store.get_meta(&id).await.expect("meta");
    assert_eq!(
        meta.cwd,
        PathBuf::from("/tmp/mock-workspaces").join(id.to_string())
    );
    assert_eq!(meta.base_cwd, Some(PathBuf::from("/repo")));
    assert_eq!(meta.workspace_id, Some(format!("ws-{id}")));
    assert_eq!(meta.isolation, Some(IsolationPolicy::Required));
    assert_eq!(mock.provision_calls(), 1);
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
        .expect("optional isolation never fails");

    let meta = store.get_meta(&id).await.expect("meta");
    assert_eq!(meta.cwd, PathBuf::from("/repo"));
    assert_eq!(meta.workspace_id, None);
    assert_eq!(meta.base_cwd, None);
    assert_eq!(meta.isolation, Some(IsolationPolicy::Optional));
    assert_eq!(provisioned_events(&store, &id).await, 0);
}

#[tokio::test]
async fn required_isolation_fails_when_backend_cannot_provision() {
    let store = Arc::new(MemoryStore::new());
    let mock = Arc::new(MockWorkspaces::unavailable());
    let agent = build_agent(store.clone(), Some(mock), Vec::new());

    let result = agent
        .create_session(params("/repo", Some(IsolationPolicy::Required)))
        .await;
    assert!(
        result.is_err(),
        "required isolation must fail when unprovisionable"
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
        "required isolation needs a configured backend"
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

    let meta = store.get_meta(&id).await.expect("meta");
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
        .expect("role-required isolation provisions");

    let meta = store.get_meta(&id).await.expect("meta");
    assert_eq!(meta.workspace_id, Some(format!("ws-{id}")));
    assert_eq!(meta.isolation, Some(IsolationPolicy::Required));
    assert_eq!(mock.provision_calls(), 1);
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
    assert_ne!(
        store.get_meta(&id).await.expect("meta").cwd,
        PathBuf::from("/repo")
    );

    agent.resume_session(&id).await.expect("resume");

    let meta = store.get_meta(&id).await.expect("meta");
    assert_eq!(
        meta.cwd,
        PathBuf::from("/repo"),
        "cwd repointed to the base on resume"
    );
}

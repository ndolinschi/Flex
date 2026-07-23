use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::{ModelRef, NewSessionParams};
use agentloop_core::{Agent, SessionStore};
use agentloop_loop::NativeAgentBuilder;
use agentloop_loop::roles::RoleSpec;
use agentloop_session::MemoryStore;

fn params(role: Option<&str>) -> NewSessionParams {
    NewSessionParams {
        cwd: Some(PathBuf::from("/repo")),
        role: role.map(str::to_owned),
        ..NewSessionParams::default()
    }
}

#[tokio::test]
async fn role_is_recorded_and_selects_the_role_chain() {
    let store = Arc::new(MemoryStore::new());
    let bot = RoleSpec {
        models: vec![ModelRef::from("bot-model")],
        ..RoleSpec::new("bot")
    };
    let agent = NativeAgentBuilder::new(store.clone())
        .system_prompt("test agent")
        .roles(vec![bot])
        .build();

    let id = agent
        .create_session(params(Some("bot")))
        .await
        .expect("known role creates");

    let meta = store.get_meta(&id).await.expect("meta");
    assert_eq!(meta.role.as_deref(), Some("bot"));
    assert_eq!(meta.model, Some(ModelRef::from("bot-model")));
}

#[tokio::test]
async fn unknown_role_is_rejected() {
    let store = Arc::new(MemoryStore::new());
    let agent = NativeAgentBuilder::new(store)
        .system_prompt("test agent")
        .build();

    let result = agent.create_session(params(Some("no-such-role"))).await;
    assert!(result.is_err(), "unknown role must be rejected");
}

#[tokio::test]
async fn no_role_keeps_the_main_defaults() {
    let store = Arc::new(MemoryStore::new());
    let agent = NativeAgentBuilder::new(store.clone())
        .system_prompt("test agent")
        .build();

    let id = agent.create_session(params(None)).await.expect("session");
    let meta = store.get_meta(&id).await.expect("meta");
    assert_eq!(meta.role, None);
}

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use agentloop_contracts::{AgentEvent, PeerMessageId, SessionId, ToolOutput, ToolResultBlock};
use agentloop_core::{
    PermissionHint, SessionStore, StoreError, Tool, ToolCategory, ToolContext, ToolDescriptor,
    ToolError,
};

use crate::fs::schema_of;

#[derive(Debug, Clone)]
pub struct PeerEnvelope {
    pub id: PeerMessageId,
    pub from: SessionId,
    pub content: String,
    pub about_path: Option<String>,
    pub thread_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct PeerMailbox {
    inner: Mutex<HashMap<SessionId, VecDeque<PeerEnvelope>>>,
}

impl PeerMailbox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn send(&self, to: SessionId, env: PeerEnvelope) {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.entry(to).or_default().push_back(env);
    }

    pub fn drain(&self, session: &SessionId) -> Vec<PeerEnvelope> {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.remove(session)
            .map(|q| q.into_iter().collect())
            .unwrap_or_default()
    }

    pub fn peek(&self, session: &SessionId) -> Vec<PeerEnvelope> {
        let map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.get(session)
            .map(|q| q.iter().cloned().collect())
            .unwrap_or_default()
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct PeerInfo {
    id: String,
    title: Option<String>,
    role: Option<String>,
    parent_id: Option<String>,
    cwd: String,
    depth: u8,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetActiveAgentsInput {
    #[serde(default)]
    about_path: Option<String>,
}

pub struct GetActiveAgentsTool {
    store: Arc<dyn SessionStore>,
}

impl GetActiveAgentsTool {
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for GetActiveAgentsTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "GetActiveAgents".to_owned(),
            description: "List peer agent sessions that are working in the same project as the \
                          caller. Returns each peer's session id, title, role, parent id, \
                          working directory, and spawn depth. Call this before editing a file \
                          that another agent may be touching concurrently — if a peer is found, \
                          use `SendMessage` to coordinate before writing. Filter with \
                          `about_path` to find agents specifically active on a given file or \
                          directory."
                .to_owned(),
            input_schema: schema_of::<GetActiveAgentsInput>(),
            read_only: true,
            category: ToolCategory::Agent,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: GetActiveAgentsInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "`GetActiveAgents` input must be {{\"about_path\": \"optional/path\"}}: {err}."
            ))
        })?;

        let current_meta = self
            .store
            .get_meta(&ctx.session_id)
            .await
            .map_err(|err| match err {
                StoreError::SessionNotFound(_) => ToolError::Execution(format!(
                    "Current session {} not found in store.",
                    ctx.session_id
                )),
                other => ToolError::Execution(format!("Store error: {other}")),
            })?;

        let current_root = current_meta.base_cwd.as_ref().unwrap_or(&current_meta.cwd);
        let current_root_str = current_root.to_string_lossy();

        let all_sessions = self
            .store
            .list()
            .await
            .map_err(|err| ToolError::Execution(format!("Failed to list sessions: {err}")))?;

        let peers: Vec<PeerInfo> = all_sessions
            .into_iter()
            .filter(|meta| {
                if meta.id == ctx.session_id {
                    return false;
                }
                let root = meta.base_cwd.as_ref().unwrap_or(&meta.cwd);
                if root.to_string_lossy() != current_root_str {
                    return false;
                }
                if let Some(filter) = &input.about_path {
                    let cwd_str = meta.cwd.to_string_lossy();
                    let base_str = meta
                        .base_cwd
                        .as_ref()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if !cwd_str.contains(filter.as_str())
                        && !base_str.contains(filter.as_str())
                        && !filter.contains(cwd_str.as_ref())
                    {
                        return false;
                    }
                }
                true
            })
            .map(|meta| PeerInfo {
                id: meta.id.0.clone(),
                title: meta.title.clone(),
                role: meta.role.clone(),
                parent_id: meta.parent_id.as_ref().map(|id| id.0.clone()),
                cwd: meta.cwd.to_string_lossy().into_owned(),
                depth: meta.depth,
            })
            .collect();

        let count = peers.len();
        let json = serde_json::to_value(&peers).unwrap_or(serde_json::Value::Array(vec![]));
        Ok(ToolOutput {
            content: vec![
                ToolResultBlock::markdown(format!(
                    "{count} peer agent(s) found in the same project."
                )),
                ToolResultBlock::Json { value: json },
            ],
            is_error: false,
            structured: Some(serde_json::json!({ "peers": peers })),
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SendMessageInput {
    to: String,
    content: String,
    #[serde(default)]
    about_path: Option<String>,
    #[serde(default)]
    thread_id: Option<String>,
}

pub struct SendMessageTool {
    mailbox: Arc<PeerMailbox>,
}

impl SendMessageTool {
    pub fn new(mailbox: Arc<PeerMailbox>) -> Self {
        Self { mailbox }
    }
}

#[async_trait]
impl Tool for SendMessageTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "SendMessage".to_owned(),
            description: "Send a coordination message to a peer agent session. Use the \
                          session id returned by `GetActiveAgents` as `to`. The recipient \
                          reads it with `GetMessages`. Typical uses: notify a peer before \
                          editing a shared file (`about_path`), ask it to pause on a \
                          conflicting change, or share an intermediate result. Use \
                          `thread_id` to link replies. This tool is read-only — it does not \
                          modify the file system."
                .to_owned(),
            input_schema: schema_of::<SendMessageInput>(),
            read_only: true,
            category: ToolCategory::Agent,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: SendMessageInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "`SendMessage` input must be {{\"to\": \"<session-id>\", \"content\": \"...\", \
                 \"about_path\": \"optional\", \"thread_id\": \"optional\"}}: {err}."
            ))
        })?;
        if input.content.trim().is_empty() {
            return Err(ToolError::InvalidInput(
                "`content` cannot be empty.".to_owned(),
            ));
        }

        let to = SessionId(input.to.clone());
        let id = PeerMessageId::generate();

        let envelope = PeerEnvelope {
            id: id.clone(),
            from: ctx.session_id.clone(),
            content: input.content.clone(),
            about_path: input.about_path.clone(),
            thread_id: input.thread_id.clone(),
        };

        self.mailbox.send(to.clone(), envelope);

        ctx.events.emit(AgentEvent::PeerMessage {
            id,
            from: ctx.session_id.clone(),
            to: Some(to.clone()),
            thread_id: input.thread_id,
            content: input.content,
            about_path: input.about_path,
        });

        Ok(ToolOutput {
            content: vec![ToolResultBlock::markdown(format!(
                "Message delivered to session `{to}`."
            ))],
            is_error: false,
            structured: None,
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetMessagesInput {
    #[serde(default = "default_drain")]
    drain: bool,
}

fn default_drain() -> bool {
    true
}

#[derive(Debug, Serialize, JsonSchema)]
struct MessageItem {
    id: String,
    from: String,
    content: String,
    about_path: Option<String>,
    thread_id: Option<String>,
}

pub struct GetMessagesTool {
    mailbox: Arc<PeerMailbox>,
}

impl GetMessagesTool {
    pub fn new(mailbox: Arc<PeerMailbox>) -> Self {
        Self { mailbox }
    }
}

#[async_trait]
impl Tool for GetMessagesTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "GetMessages".to_owned(),
            description: "Read pending inbound peer messages for this session. By default \
                          (`drain: true`) the messages are consumed; set `drain: false` to \
                          peek without removing them. Call after `GetActiveAgents` when you \
                          suspect a peer may have sent coordination messages, or at the start \
                          of each turn when multi-agent coordination is active. Returns an \
                          empty list when the inbox is clear."
                .to_owned(),
            input_schema: schema_of::<GetMessagesInput>(),
            read_only: true,
            category: ToolCategory::Agent,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: GetMessagesInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "`GetMessages` input must be {{\"drain\": true|false}}: {err}."
            ))
        })?;

        let envelopes = if input.drain {
            self.mailbox.drain(&ctx.session_id)
        } else {
            self.mailbox.peek(&ctx.session_id)
        };

        let items: Vec<MessageItem> = envelopes
            .into_iter()
            .map(|env| MessageItem {
                id: env.id.0,
                from: env.from.0,
                content: env.content,
                about_path: env.about_path,
                thread_id: env.thread_id,
            })
            .collect();

        let count = items.len();
        let json = serde_json::to_value(&items).unwrap_or(serde_json::Value::Array(vec![]));
        Ok(ToolOutput {
            content: vec![
                ToolResultBlock::markdown(format!("{count} inbound message(s).")),
                ToolResultBlock::Json { value: json },
            ],
            is_error: false,
            structured: Some(serde_json::json!({ "messages": items })),
        })
    }
}

#[cfg(test)]
mod mailbox_tests {
    use super::*;

    fn session(s: &str) -> SessionId {
        SessionId(s.to_owned())
    }

    fn envelope(from: &str, content: &str) -> PeerEnvelope {
        PeerEnvelope {
            id: PeerMessageId::generate(),
            from: session(from),
            content: content.to_owned(),
            about_path: None,
            thread_id: None,
        }
    }

    #[test]
    fn drain_removes_messages() {
        let mb = PeerMailbox::new();
        let alice = session("alice");
        let bob = session("bob");

        mb.send(bob.clone(), envelope("alice", "hello"));
        mb.send(bob.clone(), envelope("alice", "world"));

        let drained = mb.drain(&bob);
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].content, "hello");
        assert_eq!(drained[1].content, "world");

        assert!(mb.drain(&bob).is_empty());
        assert!(mb.drain(&alice).is_empty());
    }

    #[test]
    fn peek_does_not_remove_messages() {
        let mb = PeerMailbox::new();
        let bob = session("bob");

        mb.send(bob.clone(), envelope("alice", "ping"));

        let peeked = mb.peek(&bob);
        assert_eq!(peeked.len(), 1);
        assert_eq!(peeked[0].content, "ping");

        let drained = mb.drain(&bob);
        assert_eq!(drained.len(), 1);
    }

    #[test]
    fn empty_inbox_returns_empty_vec() {
        let mb = PeerMailbox::new();
        assert!(mb.drain(&session("nobody")).is_empty());
        assert!(mb.peek(&session("nobody")).is_empty());
    }

    #[test]
    fn multiple_senders_to_same_recipient() {
        let mb = PeerMailbox::new();
        let bob = session("bob");

        mb.send(bob.clone(), envelope("alice", "from alice"));
        mb.send(bob.clone(), envelope("carol", "from carol"));

        let messages = mb.drain(&bob);
        assert_eq!(messages.len(), 2);
        let contents: Vec<&str> = messages.iter().map(|m| m.content.as_str()).collect();
        assert!(contents.contains(&"from alice"));
        assert!(contents.contains(&"from carol"));
    }
}

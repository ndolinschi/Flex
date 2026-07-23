use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use agentloop_contracts::{PermissionDecisionKind, PermissionRequestId};

mod routine;

pub use routine::{RoutineError, RoutineRunRecord, RoutineSpec, RoutineStore, RoutineTrigger};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatKey {
    pub channel: String,
    pub chat_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Inbound {
    Message {
        chat: ChatKey,
        user: String,
        text: String,
    },
    PermissionReply {
        chat: ChatKey,
        id: PermissionRequestId,
        decision: PermissionDecisionKind,
    },
}

#[derive(Debug, Clone)]
pub enum Outbound {
    Text(String),
    PermissionRequest {
        id: PermissionRequestId,
        title: String,
        detail: Option<String>,
        options: Vec<PermissionDecisionKind>,
    },
    Status(String),
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ChannelError {
    #[error("channel auth failed: {0}")]
    Auth(String),
    #[error("{0}")]
    Transport(String),
    #[error("listen loop ended: {0}")]
    Closed(String),
}

#[async_trait]
pub trait Channel: Send + Sync {
    fn id(&self) -> &'static str;

    async fn listen(&self, tx: mpsc::Sender<Inbound>) -> Result<(), ChannelError>;

    async fn send(&self, chat: &ChatKey, msg: Outbound) -> Result<(), ChannelError>;
}

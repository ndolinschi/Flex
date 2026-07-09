//! The `Channel` trait and wire types: serve agent sessions over chat
//! platforms (Telegram, Slack, Discord, …).
//!
//! A channel is a dumb adapter, mirroring how providers wrap LLM APIs: it
//! turns platform webhooks/long-polls into a normalized [`Inbound`] stream and
//! renders normalized [`Outbound`] items back into platform messages. All
//! routing intelligence (chat → session mapping, permission relay) lives in
//! the gateway crate.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use agentloop_contracts::{PermissionDecisionKind, PermissionRequestId};

mod routine;

pub use routine::{RoutineError, RoutineRunRecord, RoutineSpec, RoutineStore, RoutineTrigger};

/// Stable identity of one conversation on one platform. The gateway maps each
/// key to a persistent session id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatKey {
    /// Channel id (`"telegram"`, `"slack"`, …).
    pub channel: String,
    /// Platform-specific conversation id.
    pub chat_id: String,
    /// Thread within the conversation, when the platform has them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

/// One normalized event arriving from a platform.
#[derive(Debug, Clone)]
pub enum Inbound {
    /// A user message the agent should handle as a prompt.
    Message {
        chat: ChatKey,
        /// Platform user id or handle, for logs and access control.
        user: String,
        text: String,
    },
    /// A structured reply to a pending permission request (e.g. an inline
    /// button press).
    PermissionReply {
        chat: ChatKey,
        id: PermissionRequestId,
        decision: PermissionDecisionKind,
    },
}

/// One normalized item to render into a platform message.
#[derive(Debug, Clone)]
pub enum Outbound {
    /// Assistant text (markdown; channels degrade formatting as needed).
    Text(String),
    /// A pending permission request, rendered with interactive options where
    /// the platform supports them (falling back to a textual instruction).
    PermissionRequest {
        id: PermissionRequestId,
        title: String,
        detail: Option<String>,
        options: Vec<PermissionDecisionKind>,
    },
    /// A short status line (turn started/finished, errors).
    Status(String),
}

/// Channel failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ChannelError {
    /// The platform rejected our credentials.
    #[error("channel auth failed: {0}")]
    Auth(String),
    /// Transport-level failure (network, serialization).
    #[error("{0}")]
    Transport(String),
    /// The listen loop ended unexpectedly.
    #[error("listen loop ended: {0}")]
    Closed(String),
}

/// A chat platform adapter. Implementations own all platform I/O.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Stable channel id (`"telegram"`, `"slack"`, …), used in [`ChatKey`]s.
    fn id(&self) -> &'static str;

    /// Run the inbound loop, pushing normalized events into `tx` until the
    /// sender closes or an unrecoverable error occurs. Implementations must
    /// be cancel-safe (dropping the future stops the loop).
    async fn listen(&self, tx: mpsc::Sender<Inbound>) -> Result<(), ChannelError>;

    /// Render one outbound item into `chat`.
    async fn send(&self, chat: &ChatKey, msg: Outbound) -> Result<(), ChannelError>;
}

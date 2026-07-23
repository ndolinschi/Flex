use std::sync::Arc;

use async_trait::async_trait;

use agentloop_contracts::{HookPoint, PromptInput, SessionId, ToolCall, ToolOutput, TurnId};

use crate::store::SessionStore;

#[derive(Debug, thiserror::Error)]
#[error("hook failure at {point:?}: {message}")]
pub struct HookError {
    pub point: HookPoint,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    Continue,
    Block { reason: String },
    Mutated,
}

pub enum HookData<'a> {
    Session,
    UserPrompt {
        input: &'a mut PromptInput,
    },
    ToolUse {
        call: &'a mut ToolCall,
    },
    ToolResult {
        call: &'a ToolCall,
        output: &'a mut ToolOutput,
    },
    Stop {
        continuation: &'a mut Option<String>,
    },
    Compact,
    Subagent {
        child: &'a SessionId,
    },
}

pub struct HookContext<'a> {
    pub session_id: &'a SessionId,
    pub turn_id: Option<&'a TurnId>,
    pub data: HookData<'a>,
    pub store: Option<Arc<dyn SessionStore>>,
    pub events: Option<crate::EventSink>,
}

#[async_trait]
pub trait Hook: Send + Sync {
    fn interests(&self) -> &[HookPoint];

    async fn on(
        &self,
        point: HookPoint,
        ctx: &mut HookContext<'_>,
    ) -> Result<HookOutcome, HookError>;
}

//! The `Hook` trait: ordered interceptors at loop lifecycle points.
//! One mechanism covers gating (block a tool), rewriting (mutate arguments or
//! results), and continuation (inject a follow-up on stop).

use std::sync::Arc;

use async_trait::async_trait;

use agentloop_contracts::{HookPoint, PromptInput, SessionId, ToolCall, ToolOutput, TurnId};

use crate::store::SessionStore;

/// Hook failures abort the current operation with an explanation.
#[derive(Debug, thiserror::Error)]
#[error("hook failure at {point:?}: {message}")]
pub struct HookError {
    pub point: HookPoint,
    pub message: String,
}

/// What a hook decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    Continue,
    /// Stop the operation. For `PreToolUse` this denies the call; for
    /// `UserPromptSubmit` it rejects the prompt.
    Block {
        reason: String,
    },
    /// The hook mutated data in the context (recorded in the event stream).
    Mutated,
}

/// Point-specific mutable data a hook may inspect or rewrite.
pub enum HookData<'a> {
    Session,
    UserPrompt {
        input: &'a mut PromptInput,
    },
    /// Before execution: the call (input may be rewritten).
    ToolUse {
        call: &'a mut ToolCall,
    },
    /// After execution: the finished call and its rewritable output.
    ToolResult {
        call: &'a ToolCall,
        output: &'a mut ToolOutput,
    },
    /// The model produced no tool calls; a hook may inject a continuation
    /// prompt to keep the turn going.
    Stop {
        continuation: &'a mut Option<String>,
    },
    Compact,
    Subagent {
        child: &'a SessionId,
    },
}

/// Ambient context for a hook invocation.
pub struct HookContext<'a> {
    pub session_id: &'a SessionId,
    pub turn_id: Option<&'a TurnId>,
    pub data: HookData<'a>,
    /// Read access to the current session's own log (e.g. to check for a
    /// prior tool call's recorded outcome), when the caller has one to give.
    /// `None` in contexts with no store (most unit tests); hooks that need it
    /// degrade gracefully. Hooks never write through this — mutation goes
    /// through `data`.
    pub store: Option<Arc<dyn SessionStore>>,
}

/// An ordered interceptor. Hooks run in registration order; the first
/// `Block` wins and later hooks don't run.
#[async_trait]
pub trait Hook: Send + Sync {
    /// Which points this hook wants (others are skipped without a call).
    fn interests(&self) -> &[HookPoint];

    async fn on(
        &self,
        point: HookPoint,
        ctx: &mut HookContext<'_>,
    ) -> Result<HookOutcome, HookError>;
}

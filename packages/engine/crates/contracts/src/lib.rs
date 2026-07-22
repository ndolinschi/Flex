//! Canonical data model for the agent-loop engine.
//!
//! This crate is the hub every other crate depends on. It contains only pure
//! data and pure functions: the unified event stream (content blocks, events,
//! tool calls), capability declarations, errors, the transcript reducer, and
//! the markdown projection. No I/O, no async runtime — it must stay
//! wasm-compilable so future clients can run the reducer themselves.
//!
//! Wire-format rules (enforced across the workspace):
//! - Every public type here derives `Serialize`, `Deserialize`, `JsonSchema`.
//! - Changes within a protocol version are additive only: new enum variants,
//!   new optional fields. Consumers route unknown event kinds to
//!   [`AgentEvent::Unknown`] via [`AgentEvent::from_json_lenient`].
//! - Public enums are `#[non_exhaustive]`; downstream matches need a wildcard.

pub mod branding;
pub mod capability;
pub mod checkpoint;
pub mod content;
pub mod error;
pub mod event;
pub mod goal;
pub mod hello;
pub mod hook;
pub mod ids;
pub mod markdown;
pub mod observe;
pub mod permission;
pub mod pricing;
pub mod reduce;
pub mod request;
pub mod session;
pub mod time;
pub mod tool_call;
pub mod verify;
pub mod workspace;

pub use capability::{
    AgentCaps, AgentInfo, AttachmentCaps, CancelSupport, CommandInfo, CommandSource,
    McpPassthrough, ModeInfo, ModelDiscovery, ModelInfo, ModelRef, PermissionCaps, ProviderCaps,
    ResumeSupport, StreamingGranularity,
};
pub use checkpoint::{CheckpointLabel, CheckpointRef};
pub use content::{BlobSource, ContentBlock, Message, Role, ToolResultBlock};
pub use error::{EngineError, ErrorCode, Provenance};
pub use event::{AgentEvent, ExecStream, SessionEvent};
pub use goal::{GoalOutcome, GoalSpec, GoalStopReason};
pub use hello::{EngineIdentity, Hello, PROTOCOL_VERSION};
pub use hook::{HookOutcomeKind, HookPoint};
pub use ids::{
    MessageId, ModeSwitchId, PeerMessageId, PermissionRequestId, ProviderId, QuestionId, SessionId,
    ToolCallId, TurnId,
};
pub use permission::{
    Answer, PermissionDecision, PermissionDecisionKind, PermissionMode, PermissionRule, Question,
    QuestionOption, RuleEffect,
};
pub use pricing::{ModelPrice, price_for};
pub use reduce::{Transcript, TranscriptBlock, TranscriptItem, reduce};
pub use request::{
    Effort, ExpandedCommand, NewSessionParams, PromptInput, ThinkingConfig, TurnOptions,
};
pub use session::{
    CompactionMode, CompactionSummary, PlanEntry, PlanStatus, SessionMeta, SessionMetaPatch,
    StopReason, TokenUsage, TurnStopReason, TurnSummary,
};
pub use time::now_ms;
pub use tool_call::{ToolCall, ToolCallOrigin, ToolCallStatus, ToolCallTiming, ToolOutput};
pub use verify::{VerdictOutcome, VerificationVerdict};
pub use workspace::{IntegrationOutcome, IsolationPolicy};

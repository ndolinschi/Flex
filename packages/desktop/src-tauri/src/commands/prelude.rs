//! Shared imports for command modules.
#![allow(unused_imports)]

pub(crate) use std::path::PathBuf;
pub(crate) use std::sync::Arc;

pub(crate) use agentloop_channel::{RoutineSpec, RoutineStore, RoutineTrigger};
pub(crate) use agentloop_contracts::{
    Answer, BlobSource, CommandInfo, ContentBlock, Effort, GoalSpec, IntegrationOutcome,
    IsolationPolicy, Message, ModelRef, NewSessionParams, PermissionDecision, PermissionMode,
    PermissionRequestId, PromptInput, QuestionId, SessionEvent, SessionId, SessionMeta,
    SessionMetaPatch, TurnOptions, TurnSummary,
};
pub(crate) use agentloop_core::{
    BackgroundEntrySummary, ChatRequest, ProviderStreamEvent, WorkspaceStatus,
};
pub(crate) use agentloop_sdk::mcp::McpToolClient;
pub(crate) use agentloop_sdk::routines::{default_routines_dir, FileRoutineStore, RoutineRunner};
pub(crate) use agentloop_sdk::EngineService;
pub(crate) use futures::StreamExt;
pub(crate) use serde::{Deserialize, Serialize};
pub(crate) use tauri::{AppHandle, Emitter, Manager, State};
pub(crate) use tokio_util::sync::CancellationToken;

pub(crate) use crate::compose::build_service;
pub(crate) use crate::config::{
    normalize_inline_model_id, persist_config, InlineCompletionPrefs, ProviderConfig,
    ProviderConfigView, ProviderProfile, ProviderProfileInput, ProviderProfileView,
    SaveProviderConfigInput,
};
pub(crate) use crate::error::{DesktopError, DesktopResult};
pub(crate) use crate::secrets::SecretStorageMode;
pub(crate) use crate::state::AppState;

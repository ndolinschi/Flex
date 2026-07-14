//! Engine service front door.
//!
//! This crate owns the ready runtime boundary above concrete agents: handshake,
//! session operations, replay/materialized items, and native-loop composition
//! over a *prebuilt* [`ProviderRegistry`]. It is provider-agnostic: it never
//! constructs concrete providers or delegators — the `providers` facade
//! resolves those and hands the registry (plus a default model) to
//! [`EngineService::native`].

mod background;
mod error;
mod goal;
mod native;
mod options;
mod paths;
mod replay;
mod service;
mod session;
mod turn_api;
mod verify;
mod workspace;

pub use error::{EngineResult, EngineServiceError};
pub use options::{EngineConfig, OutputVerbosity};
pub use service::EngineService;

pub use agentloop_hooks::{CheckSpec, DiagnosticsConfig, FormatterSpec};
pub use agentloop_loop::roles::{RoleError, RoleRegistry, RoleSpec, RoleToolProfile, valid_name};

#[cfg(test)]
mod tests;

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

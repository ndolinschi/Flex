//! Cursor delegator profile scaffolding.
//!
//! Cursor integration is not registered as a runtime adapter yet. This crate
//! keeps the future profile/config boundary explicit and returns actionable
//! errors if code tries to build a launch spec before a real bridge exists.

mod config;

pub use config::{CursorConfigError, CursorIntegrationKind, CursorLaunchConfig, CursorProfile};

pub const CURSOR_AGENT_ID: &str = "cursor";

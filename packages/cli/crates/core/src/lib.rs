//! Engine composition and orchestration for the interactive CLI.
//!
//! This crate is the CLI's composition root over the engine workspace: it
//! builds [`agentloop_engine::EngineService`]s for the native loop and the
//! external-agent delegators, drives session lifecycles, aggregates the
//! model catalog for pickers, and orchestrates the GitHub Copilot device-flow
//! login. It contains no terminal code — the TUI crate is a pure renderer on
//! top of the types exposed here.

pub mod auth;
pub mod catalog;
pub mod connect;
pub mod controller;
pub mod engines;
pub mod mcp_store;
pub mod prefs;

pub use auth::{AuthError, LoginEvent, has_copilot_credentials, login_copilot};
pub use catalog::{CatalogEntry, ModelCatalog};
pub use connect::validate_provider;
pub use controller::SessionController;
pub use engines::{AgentKind, EngineHub, HubError};
pub use mcp_store::{
    InstallTarget, InstalledMcpServer, McpInstallSource, McpRegistryEntry, McpStore, McpStoreError,
    mcp_path, mcp_servers_dir, parse_install_target, registry,
};
pub use prefs::{
    CliPrefs, ModelEntry, PrefsError, ProviderConfig, config_dir, config_path, custom_specs,
    model_in_catalog, model_provider_available, resolve_api_key, resolve_stored_model,
};

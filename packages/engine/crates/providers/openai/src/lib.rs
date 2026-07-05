//! OpenAI provider client.
//!
//! The public surface is provider configuration plus the [`OpenAiProvider`].
//! Chat Completions wire types are private and normalized into
//! `ProviderStreamEvent` before leaving this crate.

pub mod compat;
mod config;
mod provider;
mod wire;

pub use config::{DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL, OPENAI_PROVIDER_ID, OpenAiConfig};
pub use provider::OpenAiProvider;

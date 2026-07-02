//! Anthropic provider client.
//!
//! The public surface is provider configuration plus the [`AnthropicProvider`].
//! Messages API wire types are private and normalized into
//! `ProviderStreamEvent` before leaving this crate.

mod config;
mod provider;
mod wire;

pub use config::{
    ANTHROPIC_PROVIDER_ID, AnthropicConfig, DEFAULT_ANTHROPIC_BASE_URL, DEFAULT_ANTHROPIC_MODEL,
};
pub use provider::AnthropicProvider;

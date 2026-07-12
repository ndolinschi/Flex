//! OpenAI provider client.
//!
//! The public surface is provider configuration plus the [`OpenAiProvider`].
//! Chat Completions wire types are private and normalized into
//! `ProviderStreamEvent` before leaving this crate.

pub mod compat;
mod config;
mod oauth;
mod provider;
mod wire;

pub use config::{DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL, OPENAI_PROVIDER_ID, OpenAiConfig};
pub use oauth::{
    OpenAiOAuthMethod, OpenAiOAuthStart, OpenAiOAuthTokens, oauth_tokens_discoverable,
    resolve_oauth_access_token, start_oauth, store_oauth_tokens,
};
pub use provider::OpenAiProvider;

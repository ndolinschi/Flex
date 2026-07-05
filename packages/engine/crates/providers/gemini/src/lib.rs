//! Gemini provider client.
//!
//! The public surface is provider configuration plus the [`GeminiProvider`].
//! GenerateContent wire types are private and normalized into
//! `ProviderStreamEvent` before leaving this crate.

mod config;
mod provider;
mod wire;

pub use config::{DEFAULT_GEMINI_BASE_URL, DEFAULT_GEMINI_MODEL, GEMINI_PROVIDER_ID, GeminiConfig};
pub use provider::GeminiProvider;

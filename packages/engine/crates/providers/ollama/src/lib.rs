//! Ollama provider client.
//!
//! The public surface is provider configuration plus the [`OllamaProvider`].
//! Ollama chat wire types are private and normalized into
//! `ProviderStreamEvent` before leaving this crate.

mod config;
mod provider;
mod wire;

pub use config::{DEFAULT_OLLAMA_HOST, DEFAULT_OLLAMA_MODEL, OLLAMA_PROVIDER_ID, OllamaConfig};
pub use provider::OllamaProvider;

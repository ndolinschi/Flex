mod config;
mod provider;
mod wire;

pub use config::{DEFAULT_OLLAMA_HOST, DEFAULT_OLLAMA_MODEL, OLLAMA_PROVIDER_ID, OllamaConfig};
pub use provider::OllamaProvider;

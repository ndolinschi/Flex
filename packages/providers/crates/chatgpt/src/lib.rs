//! ChatGPT Plus/Pro subscription provider.
//!
//! Speaks the Codex Responses backend used by ChatGPT subscription OAuth
//! (`https://chatgpt.com/backend-api/codex/responses`), reusing the ChatGPT
//! OAuth flow from [`agentloop_provider_openai`]. Distinct from the Platform
//! API-key [`agentloop_provider_openai::OpenAiProvider`] Chat Completions path.

mod config;
mod models;
mod provider;
mod wire;

pub use config::{CHATGPT_PROVIDER_ID, CODEX_RESPONSES_URL, ChatgptConfig, DEFAULT_CHATGPT_MODEL};
pub use models::static_models;
pub use provider::ChatgptProvider;

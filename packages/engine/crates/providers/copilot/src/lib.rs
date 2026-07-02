//! GitHub Copilot provider.
//!
//! Copilot's chat API speaks the OpenAI Chat Completions dialect at its own
//! endpoints with its own authentication chain:
//!
//! 1. A long-lived GitHub OAuth token, discovered from the environment
//!    (`COPILOT_GITHUB_TOKEN` / `GH_COPILOT_TOKEN`) or from an existing
//!    sign-in in `~/.config/github-copilot/{apps,hosts}.json` (created by
//!    VS Code, JetBrains, or the Copilot CLI).
//! 2. Exchanged at `https://api.github.com/copilot_internal/v2/token` for a
//!    short-lived bearer token that also names the API base
//!    (`https://api.githubcopilot.com` unless the account is routed
//!    elsewhere). Cached and refreshed ahead of expiry.
//! 3. Chat at `<api>/chat/completions` and models at `<api>/models`, with the
//!    Copilot integration headers, reusing the OpenAI-compatible wire layer
//!    from `agentloop-provider-openai`.

mod auth;
mod config;
mod provider;

pub use config::{
    COPILOT_PROVIDER_ID, CopilotConfig, DEFAULT_COPILOT_MODEL, DEFAULT_COPILOT_TOKEN_URL,
    FALLBACK_COPILOT_API_BASE,
};
pub use provider::CopilotProvider;

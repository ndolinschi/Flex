mod auth;
mod config;
mod device_flow;
mod provider;

pub use config::{
    COPILOT_PROVIDER_ID, CopilotConfig, DEFAULT_COPILOT_MODEL, DEFAULT_COPILOT_TOKEN_URL,
    FALLBACK_COPILOT_API_BASE, store_github_token,
};
pub use device_flow::{COPILOT_DEVICE_CLIENT_ID, DeviceAuthorization, DeviceFlow};
pub use provider::CopilotProvider;

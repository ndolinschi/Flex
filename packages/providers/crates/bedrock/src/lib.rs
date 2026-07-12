//! AWS Bedrock provider.
//!
//! Talks to the region-scoped Bedrock Runtime `converse-stream` endpoint, which
//! is model-agnostic (Claude, Llama, Nova, …) so this one crate covers every
//! Bedrock-hosted model. Auth is either a Bedrock **API key** (bearer token,
//! `AWS_BEARER_TOKEN_BEDROCK`) or **SigV4** AWS credentials — see [`BedrockAuth`].
//! Model listing is dynamic (control-plane `ListFoundationModels` /
//! `ListInferenceProfiles`) with a static fallback.
//!
//! Bedrock streams responses in the AWS **event-stream** binary framing rather
//! than SSE; [`eventstream`] decodes the frames and [`wire`] maps the Converse
//! events onto the canonical [`agentloop_core::ProviderStreamEvent`] stream.

mod config;
mod eventstream;
mod models;
mod provider;
mod sigv4;
mod wire;

pub use config::{BEDROCK_PROVIDER_ID, BedrockAuth, BedrockConfig};
pub use provider::BedrockProvider;

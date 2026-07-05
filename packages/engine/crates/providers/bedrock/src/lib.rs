//! AWS Bedrock provider.
//!
//! Talks to the region-scoped Bedrock Runtime `converse-stream` endpoint, which
//! is model-agnostic (Claude, Llama, Nova, …) so this one crate covers every
//! Bedrock-hosted model. Auth is a Bedrock **API key** (bearer token) from
//! `AWS_BEARER_TOKEN_BEDROCK` — the SigV4 credential chain (IAM keys, profiles,
//! IRSA) is a planned follow-on.
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

pub use config::{BEDROCK_PROVIDER_ID, BedrockConfig};
pub use provider::BedrockProvider;

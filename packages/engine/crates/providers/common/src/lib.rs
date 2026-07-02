//! Shared utilities for first-party provider crates.
//!
//! Provider-specific wire types stay in each provider crate. This crate only
//! holds generic pieces: standard environment loading, HTTP error mapping, and
//! Server-Sent Events decoding.

mod env;
mod http;
mod sse;

pub use env::{optional_env, required_env};
pub use http::{authenticated_request, looks_like_context_overflow, status_to_provider_error};
pub use sse::{SseDecoder, SseError, SseEvent};

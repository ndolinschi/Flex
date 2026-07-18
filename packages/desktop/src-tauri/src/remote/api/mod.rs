//! Desktop Remote Access HTTP `/v1` API (axum).

mod dto;
mod openapi;
mod routes;
mod sse;

pub use routes::{v1_router, RemoteApiState};

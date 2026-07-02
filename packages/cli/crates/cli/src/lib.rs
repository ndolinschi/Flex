//! Interactive terminal client for the agent-loop engine.
//!
//! The binary lives in `main.rs`; everything else is a library so the
//! reducer and widgets are testable without a terminal. The architecture is
//! an Elm-style reducer: [`app::App::update`] consumes [`events::AppEvent`]s
//! and returns [`events::Effect`]s, which [`runtime::EffectExecutor`]
//! executes on spawned tasks. Rendering is a pure function of [`app::App`]
//! in [`ui`].

pub mod app;
pub mod args;
pub mod chat;
pub mod clipboard;
pub mod commands;
pub mod events;
pub mod files;
pub mod input;
pub mod overlay;
pub mod runtime;
pub mod theme;
pub(crate) mod tool_output;
pub mod ui;

#[cfg(test)]
mod render_snapshots;

#[cfg(test)]
pub mod testing;

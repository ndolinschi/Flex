//! Shared process plumbing: spawn a prepared command, enforce timeout and
//! cancellation, collect output. Every backend funnels through here so the
//! cancel/timeout semantics are identical across backends.

mod background;
mod demote;
mod foreground;
mod io;
mod probe;

pub(crate) use background::spawn_background;
pub(crate) use demote::run_command_demotable;
pub(crate) use foreground::{run_command, run_command_with_sink};
pub(crate) use probe::probe_binary;

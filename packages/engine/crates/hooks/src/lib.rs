//! Post-tool-use hooks: format-on-edit and availability-gated diagnostics.
//!
//! These are the sanctioned I/O edge for post-edit tooling. `loop` calls the
//! injected [`agentloop_core::Hook`] trait; the subprocess work (formatters,
//! check commands) lives here, never in `loop`. Both hooks fire on
//! [`agentloop_contracts::HookPoint::PostToolUse`] for `Write`/`Edit` calls and
//! mutate the finished tool's [`agentloop_contracts::ToolOutput`] in place — the
//! loop feeds the rewritten output back to the model on the next iteration.
//!
//! Everything is opt-in and **availability-gated**: a formatter/check whose
//! binary does not resolve on `$PATH`, or that no configured spec matches, is a
//! silent no-op. Neither hook ever blocks or fails a turn.

mod diagnostics;
mod format;
mod injection;
mod util;

pub use diagnostics::{CheckSpec, DiagnosticsConfig, DiagnosticsHook};
pub use format::{FormatOnEditHook, FormatterSpec};
pub use injection::{InjectionFinding, InjectionScanHook, scan_text};

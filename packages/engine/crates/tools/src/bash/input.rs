//! Bash tool input schema.

use schemars::JsonSchema;
use serde::Deserialize;

/// What to do with an already-started background process, named by the id
/// returned when it was started (see [`BashInput::process_id`]).
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum BackgroundAction {
    /// Report whether the process is still running plus its recent output.
    Status,
    /// Terminate the process.
    Kill,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct BashInput {
    /// Shell command to run in the session cwd. Not required when `action`
    /// targets an already-started background process (`background_action` +
    /// `process_id`).
    pub(super) command: Option<String>,
    /// Optional timeout in milliseconds. Defaults to 30000, capped at 600000.
    /// Ignored for `run_in_background: true` (see that field).
    pub(super) timeout_ms: Option<u64>,
    /// Start long-running processes (dev servers, watchers, tail -f, etc.)
    /// in the background instead of blocking: the call returns once the
    /// process's initial output settles (a few seconds), with a process id
    /// and whatever it printed on startup. The process keeps running and
    /// keeps streaming output to the agent terminal after the call returns;
    /// use `background_action: "status"` with that id later to check on it,
    /// or `"kill"` to stop it. Defaults to `false` (blocking, byte-identical
    /// to today's behavior).
    #[serde(default)]
    pub(super) run_in_background: bool,
    /// Check on or stop a background process previously started with
    /// `run_in_background: true`. Requires `process_id`; `command` is
    /// ignored when this is set.
    pub(super) background_action: Option<BackgroundAction>,
    /// The process id returned by the `run_in_background: true` call that
    /// started it. Required with `background_action`.
    pub(super) process_id: Option<String>,
}

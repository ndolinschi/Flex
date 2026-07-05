//! Session debug logging (agent debug mode).
// Debug-only scaffold: helpers are `pub` for call sites across the crate but
// the module itself is `pub(crate)`, so `unreachable_pub` fires spuriously.
#![allow(unreachable_pub)]

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const LOG_PATH: &str =
    "/Users/nichita.home/Documents/Projects/AgenticStudio/.cursor/debug-68968a.log";

static LAST_SPLASH_HINT: Mutex<Option<usize>> = Mutex::new(None);
static LAST_PLACEHOLDER_IDX: Mutex<Option<usize>> = Mutex::new(None);

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Append one NDJSON line for hypothesis testing.
pub fn log(hypothesis_id: &str, location: &str, message: &str, data: serde_json::Value) {
    let line = serde_json::json!({
        "sessionId": "68968a",
        "hypothesisId": hypothesis_id,
        "location": location,
        "message": message,
        "data": data,
        "timestamp": timestamp_ms(),
    });
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(LOG_PATH) {
        let _ = writeln!(file, "{line}");
    }
}

/// Log when the home splash hint index changes (hypothesis A).
pub fn log_splash_hint_if_changed(spinner: usize, hint_idx: usize) {
    let mut last = LAST_SPLASH_HINT.lock().unwrap_or_else(|e| e.into_inner());
    if *last == Some(hint_idx) {
        return;
    }
    *last = Some(hint_idx);
    log(
        "A",
        "ui/mod.rs:draw_home_centered",
        "splash hint index changed",
        serde_json::json!({ "spinner": spinner, "hintIdx": hint_idx, "runId": "post-fix" }),
    );
}

/// Log when the input placeholder index changes (hypothesis B).
pub fn log_placeholder_if_changed(tick: usize, idx: usize) {
    let mut last = LAST_PLACEHOLDER_IDX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if *last == Some(idx) {
        return;
    }
    *last = Some(idx);
    log(
        "B",
        "input.rs:rotate_placeholder",
        "placeholder index changed",
        serde_json::json!({ "tick": tick, "placeholderIdx": idx }),
    );
}

/// Log paste event before any slow work (hypothesis F).
pub fn log_paste_begin(bytes: usize) {
    log(
        "F",
        "app.rs:Paste",
        "paste begin",
        serde_json::json!({ "bytes": bytes, "runId": "post-fix" }),
    );
}

/// Log paste handling timing and path (hypotheses F/G).
pub fn log_paste(
    bytes: usize,
    line_count: usize,
    count_us: u128,
    collapsed: bool,
    textarea_lines: usize,
) {
    log(
        if collapsed { "F" } else { "G" },
        "input.rs:paste",
        "paste handled",
        serde_json::json!({
            "bytes": bytes,
            "lineCount": line_count,
            "linesCountUs": count_us,
            "collapsed": collapsed,
            "textareaLines": textarea_lines,
        }),
    );
}

/// Log submit expansion timing (hypothesis H).
pub fn log_submit_expand(input_bytes: usize, output_bytes: usize, expand_us: u128, blocks: usize) {
    log(
        "H",
        "input.rs:submit",
        "expand pasted blocks",
        serde_json::json!({
            "inputBytes": input_bytes,
            "outputBytes": output_bytes,
            "expandUs": expand_us,
            "pastedBlocks": blocks,
        }),
    );
}

/// Log refresh_popup cost after paste (hypothesis I).
pub fn log_refresh_popup(text_bytes: usize, refresh_us: u128) {
    log(
        "I",
        "input.rs:refresh_popup",
        "refresh after paste",
        serde_json::json!({ "textBytes": text_bytes, "refreshUs": refresh_us }),
    );
}

//! Session debug logging (NDJSON append). Folded in editor via region markers.

use serde_json::Value;

const LOG_PATH: &str = "/Users/ndolinschi/Documents/Apps/AgenticStudio/.cursor/debug-79ecfd.log";

// #region agent log
pub(crate) fn agent_debug_log(hypothesis_id: &str, location: &str, message: &str, data: Value) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_PATH)
    {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let payload = serde_json::json!({
            "sessionId": "79ecfd",
            "timestamp": timestamp,
            "hypothesisId": hypothesis_id,
            "location": location,
            "message": message,
            "data": data,
            "runId": "decline-repro",
        });
        let _ = writeln!(file, "{payload}");
    }
}
// #endregion

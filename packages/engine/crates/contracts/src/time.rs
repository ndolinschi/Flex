//! Wall-clock helper. Timestamps on the wire are unix epoch milliseconds
//! (`u64`) everywhere — no chrono types in contracts.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current unix epoch time in milliseconds.
///
/// Saturates to 0 if the system clock reports a time before the epoch, so it
/// never panics.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

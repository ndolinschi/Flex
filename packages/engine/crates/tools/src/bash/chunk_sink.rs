//! Live `ExecChunk` sink bridging executor streams onto wire events.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use agentloop_contracts::{AgentEvent, ExecStream as WireExecStream};
use agentloop_core::{ChunkSink, ExecStream, ToolContext};

use super::MAX_OUTPUT_CHARS;

/// Build a [`ChunkSink`] that emits `AgentEvent::ExecChunk` for every
/// incremental chunk a running command produces, mapping the executor's
/// wire-format-free `agentloop_core::ExecStream` onto the wire enum
/// (`agentloop_contracts::ExecStream`) — the only layer that is allowed to
/// know about both is this one (`tools` depends on `core` and `contracts`;
/// `executors` depends on `core` alone).
///
/// Streaming stops once the running total exceeds `MAX_OUTPUT_CHARS`: the
/// executor keeps accumulating the full output for the final, still-truncated
/// `ToolOutput` (unchanged from today), but there is no point flooding live
/// subscribers past the cap the final render already enforces.
pub(super) fn exec_chunk_sink(ctx: &ToolContext) -> ChunkSink {
    let events = ctx.events.clone();
    let call_id = ctx.call_id.clone();
    let emitted = Arc::new(AtomicUsize::new(0));
    Arc::new(move |stream, text| {
        if emitted.load(Ordering::Relaxed) > MAX_OUTPUT_CHARS {
            return;
        }
        let previous = emitted.fetch_add(text.chars().count(), Ordering::Relaxed);
        if previous > MAX_OUTPUT_CHARS {
            return;
        }
        let stream = match stream {
            ExecStream::Stdout => WireExecStream::Stdout,
            ExecStream::Stderr => WireExecStream::Stderr,
            // `ExecStream` is `#[non_exhaustive]`: an unrecognized future
            // stream kind is treated as stdout rather than dropped or
            // panicking.
            _ => WireExecStream::Stdout,
        };
        events.emit(AgentEvent::ExecChunk {
            call_id: call_id.clone(),
            stream,
            text: text.to_owned(),
        });
    })
}

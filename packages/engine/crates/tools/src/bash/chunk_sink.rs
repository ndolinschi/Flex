use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use agentloop_contracts::{AgentEvent, ExecStream as WireExecStream};
use agentloop_core::{ChunkSink, ExecStream, ToolContext};

use super::MAX_OUTPUT_CHARS;

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
            _ => WireExecStream::Stdout,
        };
        events.emit(AgentEvent::ExecChunk {
            call_id: call_id.clone(),
            stream,
            text: text.to_owned(),
        });
    })
}

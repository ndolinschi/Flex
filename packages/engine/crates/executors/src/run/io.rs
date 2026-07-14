//! Stdio chunk readers shared by foreground and background runners.

use std::sync::{Arc, Mutex};

use tokio::io::AsyncReadExt;

use agentloop_core::{ChunkSink, ExecStream};

use super::background::BackgroundState;

/// Size of each incremental read. Chunk-based (not line-based): a command
/// that never emits a newline (a progress spinner, a long single-line log)
/// still streams instead of stalling until EOF.
pub(super) const CHUNK_BUF_SIZE: usize = 8 * 1024;

/// Cap on the accumulated tail buffer, in bytes. Older bytes are dropped as
/// new ones arrive (ring-buffer-by-truncation) so a chatty long-running
/// process can't grow the registry entry unbounded.
pub(super) const TAIL_BUFFER_CAP_BYTES: usize = 16 * 1024;

pub(super) async fn read_and_forward_dual<R>(
    mut reader: R,
    stream: ExecStream,
    sink: Option<ChunkSink>,
    state: Arc<Mutex<BackgroundState>>,
) -> Vec<u8>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buf = vec![0u8; CHUNK_BUF_SIZE];
    let mut acc = Vec::new();
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buf[..n];
                acc.extend_from_slice(chunk);
                if let Some(sink) = &sink {
                    let text = String::from_utf8_lossy(chunk);
                    sink(stream, &text);
                }
                let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
                state.tail.extend_from_slice(chunk);
                if state.tail.len() > TAIL_BUFFER_CAP_BYTES {
                    let overflow = state.tail.len() - TAIL_BUFFER_CAP_BYTES;
                    state.tail.drain(0..overflow);
                }
            }
            Err(_) => break,
        }
    }
    acc
}

/// Read one pipe to EOF in 8KB chunks, forwarding each chunk to `sink` (lossy
/// UTF-8) while accumulating the raw bytes for the final result.
pub(super) async fn read_and_forward<R>(
    mut reader: R,
    stream: ExecStream,
    sink: ChunkSink,
) -> Vec<u8>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buf = vec![0u8; CHUNK_BUF_SIZE];
    let mut acc = Vec::new();
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                acc.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&buf[..n]);
                sink(stream, &text);
            }
            Err(_) => break,
        }
    }
    acc
}

/// Like [`read_and_forward`], but for a detached background process: mirrors
/// each chunk into the shared, capped tail buffer (instead of an unbounded
/// accumulator returned to a waiting caller) and optionally forwards the raw
/// bytes to `initial_tx` for the caller collecting the initial-output window.
pub(super) async fn read_and_forward_background<R>(
    mut reader: R,
    stream: ExecStream,
    sink: Option<ChunkSink>,
    state: Arc<Mutex<BackgroundState>>,
    initial_tx: Option<tokio::sync::mpsc::UnboundedSender<(ExecStream, Vec<u8>)>>,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buf = vec![0u8; CHUNK_BUF_SIZE];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buf[..n];
                if let Some(sink) = &sink {
                    let text = String::from_utf8_lossy(chunk);
                    sink(stream, &text);
                }
                {
                    let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
                    state.tail.extend_from_slice(chunk);
                    if state.tail.len() > TAIL_BUFFER_CAP_BYTES {
                        let overflow = state.tail.len() - TAIL_BUFFER_CAP_BYTES;
                        state.tail.drain(0..overflow);
                    }
                }
                if let Some(tx) = &initial_tx {
                    let _ = tx.send((stream, chunk.to_vec()));
                }
            }
            Err(_) => break,
        }
    }
}

//! Temporary prompt attachment path resolution.
//!
//! `BlobSource::Path` intake belongs at the future engine boundary; for now the
//! native loop resolves paths before provider requests are built.

use std::path::Path;

use agentloop_contracts::{BlobSource, ContentBlock, PromptInput};
use agentloop_core::AgentError;
use base64::Engine as _;

const MAX_BLOB_BYTES: u64 = 5 * 1024 * 1024;

/// Resolve `BlobSource::Path` attachments to base64 before anything reaches a
/// provider. Size-capped; relative paths resolve against the session cwd.
pub(crate) async fn resolve_blob_paths(
    input: &mut PromptInput,
    cwd: &Path,
) -> Result<(), AgentError> {
    for part in &mut input.parts {
        let data = match part {
            ContentBlock::Image { data, .. } | ContentBlock::File { data, .. } => data,
            _ => continue,
        };
        if let BlobSource::Path { path } = data {
            let resolved = if path.is_absolute() {
                path.clone()
            } else {
                cwd.join(&*path)
            };
            let meta = tokio::fs::metadata(&resolved).await.map_err(|err| {
                AgentError::Other(format!(
                    "attachment not readable at {}: {err}",
                    resolved.display()
                ))
            })?;
            if meta.len() > MAX_BLOB_BYTES {
                return Err(AgentError::Other(format!(
                    "attachment {} is {} bytes; the limit is {MAX_BLOB_BYTES}",
                    resolved.display(),
                    meta.len()
                )));
            }
            let bytes = tokio::fs::read(&resolved).await.map_err(|err| {
                AgentError::Other(format!(
                    "failed to read attachment {}: {err}",
                    resolved.display()
                ))
            })?;
            *data = BlobSource::Base64 {
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
            };
        }
    }
    Ok(())
}

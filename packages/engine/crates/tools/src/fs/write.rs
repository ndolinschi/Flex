use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use super::{FsState, check_freshness, modified_time, require_absolute, schema_of};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WriteInput {
    file_path: String,
    content: String,
    create_dirs: Option<bool>,
}

pub struct WriteTool {
    state: Arc<FsState>,
}

impl WriteTool {
    pub fn new(state: Arc<FsState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "Write".to_owned(),
            description: "Write complete file contents to an absolute `file_path`. Use for new \
                          files or intentional full replacement. If the file already exists, \
                          call `Read` first; Write refuses stale overwrites. Pass \
                          `create_dirs: true` only when parent directories should be created."
                .to_owned(),
            input_schema: schema_of::<WriteInput>(),
            read_only: false,
            category: ToolCategory::Fs,
            needs_permission: PermissionHint::IfMutating,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: WriteInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `Write` must be {{\"file_path\": \"/absolute/path\", \
                 \"content\": \"...\", \"create_dirs\": <optional bool>}}: {err}."
            ))
        })?;
        let path = require_absolute(&input.file_path, &ctx.cwd)?;

        if let Ok(metadata) = tokio::fs::metadata(&path).await {
            if metadata.is_dir() {
                return Err(ToolError::InvalidInput(format!(
                    "`{}` is a directory. Pass a file path for Write.",
                    path.display()
                )));
            }
            check_freshness(&self.state, &path, modified_time(&metadata), "Write")?;
        } else if let Some(parent) = path.parent() {
            ensure_parent(parent, input.create_dirs.unwrap_or(false)).await?;
        }

        tokio::select! {
            _ = ctx.cancel.cancelled() => Err(ToolError::Cancelled),
            result = tokio::fs::write(&path, input.content.as_bytes()) => {
                result.map_err(|err| {
                    ToolError::Execution(format!("Cannot Write `{}`: {err}.", path.display()))
                })?;
                let metadata = tokio::fs::metadata(&path).await.map_err(|err| {
                    ToolError::Execution(format!(
                        "Wrote `{}` but could not stat it afterward: {err}.",
                        path.display()
                    ))
                })?;
                self.state.record_read(path.clone(), modified_time(&metadata));
                Ok(ToolOutput::text(format!(
                    "Wrote `{}` ({} bytes).",
                    path.display(),
                    metadata.len()
                )))
            }
        }
    }
}

async fn ensure_parent(parent: &Path, create_dirs: bool) -> Result<(), ToolError> {
    if parent.as_os_str().is_empty() || tokio::fs::metadata(parent).await.is_ok() {
        return Ok(());
    }
    if create_dirs {
        tokio::fs::create_dir_all(parent).await.map_err(|err| {
            ToolError::Execution(format!(
                "Cannot create parent directory `{}`: {err}.",
                parent.display()
            ))
        })?;
        return Ok(());
    }
    Err(ToolError::Execution(format!(
        "Parent directory `{}` does not exist. Create it first or retry Write with \
         `create_dirs: true`.",
        parent.display()
    )))
}

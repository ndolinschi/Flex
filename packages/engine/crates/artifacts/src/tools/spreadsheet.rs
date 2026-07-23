use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::office::{ArtifactBuildSpec, OfficeArtifact};
use crate::path::{ensure_parent, relative_from, require_absolute};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreateSpreadsheetInput {
    file_path: String,

    title: String,

    headers: Option<Vec<String>>,

    rows: Vec<Vec<String>>,

    #[serde(default = "default_true")]
    create_dirs: bool,
}

fn default_true() -> bool {
    true
}

pub struct CreateSpreadsheetTool {
    backend: Arc<dyn OfficeArtifact>,
}

impl CreateSpreadsheetTool {
    pub fn new(backend: Arc<dyn OfficeArtifact>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Tool for CreateSpreadsheetTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "CreateSpreadsheet".to_owned(),
            description: "Create an Excel spreadsheet (.xlsx) at an absolute `file_path`. \
                         Provide a sheet `title` (tab name), optional `headers` array, and \
                         `rows` (an array of string arrays). Parent directories are created \
                         by default. \
                         Do not use `Write` for .xlsx files — it would produce corrupt output. \
                         Place generated spreadsheets under `artifacts/` or `reports/` unless \
                         the user specifies otherwise."
                .to_owned(),
            input_schema: schema_of::<CreateSpreadsheetInput>(),
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
        let input: CreateSpreadsheetInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `CreateSpreadsheet` must be \
                     {{\"file_path\": \"/abs/path.xlsx\", \"title\": \"Sheet1\", \
                     \"headers\"?: [...], \"rows\": [[...], ...], \
                     \"create_dirs\"?: true}}: {err}."
            ))
        })?;

        if input.title.len() > 31 {
            return Err(ToolError::InvalidInput(format!(
                "Sheet `title` must be 31 characters or fewer (Excel limit); \
                 got {} characters.",
                input.title.len()
            )));
        }

        let path = require_absolute(&input.file_path, &ctx.cwd)?;

        if ctx.cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }

        ensure_parent(&path, input.create_dirs).await?;

        let spec = ArtifactBuildSpec::Spreadsheet {
            title: input.title.clone(),
            headers: input.headers.clone(),
            rows: input.rows.clone(),
        };

        let backend = Arc::clone(&self.backend);
        let bytes = tokio::task::spawn_blocking(move || backend.build(&spec))
            .await
            .map_err(|e| ToolError::Execution(format!("spreadsheet build panicked: {e}")))?
            .map_err(|e| ToolError::Execution(format!("spreadsheet build failed: {e}")))?;

        tokio::select! {
            _ = ctx.cancel.cancelled() => Err(ToolError::Cancelled),
            result = tokio::fs::write(&path, &bytes) => {
                result.map_err(|err| {
                    ToolError::Execution(format!(
                        "Cannot write `{}`: {err}.",
                        path.display()
                    ))
                })?;
                let rel = relative_from(&path, &ctx.cwd);
                Ok(ToolOutput {
                    content: vec![agentloop_contracts::ToolResultBlock::markdown(format!(
                        "Created spreadsheet `{}` ({} bytes).",
                        path.display(),
                        bytes.len(),
                    ))],
                    is_error: false,
                    structured: Some(serde_json::json!({
                        "relativePath": rel,
                        "title": input.title,
                        "kind": "spreadsheet",
                    })),
                })
            }
        }
    }
}

fn schema_of<T: JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(T))
        .unwrap_or_else(|_| serde_json::json!({"type": "object"}))
}

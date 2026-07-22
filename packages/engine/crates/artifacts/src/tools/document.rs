//! `CreateDocument` — writes a `.docx` file at an absolute path.

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
struct CreateDocumentInput {
    /// Absolute path for the output `.docx` file, e.g. `/project/reports/summary.docx`.
    /// Must be absolute; if you want a file under the session working directory,
    /// pass the full path (the session cwd is shown in context).
    file_path: String,

    /// Document title — used as the first paragraph (bold heading).
    title: String,

    /// Body text. Blank lines separate paragraphs. Markdown-style formatting is
    /// preserved as plain text — the model should write clean prose, not raw
    /// markdown syntax, as the output is a binary Word file.
    body: String,

    /// Create missing parent directories. Defaults to `true`.
    #[serde(default = "default_true")]
    create_dirs: bool,
}

fn default_true() -> bool {
    true
}

/// The `CreateDocument` tool.
pub struct CreateDocumentTool {
    backend: Arc<dyn OfficeArtifact>,
}

impl CreateDocumentTool {
    pub fn new(backend: Arc<dyn OfficeArtifact>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Tool for CreateDocumentTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "CreateDocument".to_owned(),
            description: "Create a Word document (.docx) at an absolute `file_path`. \
                         Provide a `title` (first bold paragraph) and `body` text where \
                         blank lines delimit paragraphs. Parent directories are created \
                         by default (`create_dirs: true`). \
                         Do not use the `Write` tool for .docx files — it would produce \
                         corrupted binary output. Place generated documents under \
                         `artifacts/` or `reports/` relative to the project root unless \
                         the user specifies otherwise."
                .to_owned(),
            input_schema: schema_of::<CreateDocumentInput>(),
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
        let input: CreateDocumentInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `CreateDocument` must be \
                     {{\"file_path\": \"/abs/path/to/file.docx\", \
                     \"title\": \"...\", \"body\": \"...\", \
                     \"create_dirs\"?: true}}: {err}."
            ))
        })?;

        let path = require_absolute(&input.file_path, &ctx.cwd)?;

        if ctx.cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }

        ensure_parent(&path, input.create_dirs).await?;

        let spec = ArtifactBuildSpec::Document {
            title: input.title.clone(),
            body: input.body.clone(),
        };

        let backend = Arc::clone(&self.backend);
        let bytes = tokio::task::spawn_blocking(move || backend.build(&spec))
            .await
            .map_err(|e| ToolError::Execution(format!("document build panicked: {e}")))?
            .map_err(|e| ToolError::Execution(format!("document build failed: {e}")))?;

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
                        "Created document `{}` ({} bytes).",
                        path.display(),
                        bytes.len(),
                    ))],
                    is_error: false,
                    structured: Some(serde_json::json!({
                        "relativePath": rel,
                        "title": input.title,
                        "kind": "document",
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

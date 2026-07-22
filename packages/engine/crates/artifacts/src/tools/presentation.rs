//! `CreatePresentation` — writes a `.pptx` file at an absolute path.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::office::{ArtifactBuildSpec, OfficeArtifact, Slide};
use crate::path::{ensure_parent, relative_from, require_absolute};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SlideInput {
    /// Slide title text.
    title: String,
    /// Bullet-point lines. Each string becomes one bullet.
    bullets: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreatePresentationInput {
    /// Absolute path for the output `.pptx` file.
    file_path: String,

    /// Presentation title (for metadata; not currently embedded in the deck).
    title: String,

    /// Ordered slides. Each slide has a `title` and a `bullets` array.
    slides: Vec<SlideInput>,

    /// Create missing parent directories. Defaults to `true`.
    #[serde(default = "default_true")]
    create_dirs: bool,
}

fn default_true() -> bool {
    true
}

/// The `CreatePresentation` tool.
pub struct CreatePresentationTool {
    backend: Arc<dyn OfficeArtifact>,
}

impl CreatePresentationTool {
    pub fn new(backend: Arc<dyn OfficeArtifact>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Tool for CreatePresentationTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "CreatePresentation".to_owned(),
            description: "Create a PowerPoint presentation (.pptx) at an absolute `file_path`. \
                         Provide a `title` and a `slides` array where each slide has a \
                         `title` string and a `bullets` string array. \
                         Parent directories are created by default. \
                         Do not use `Write` for .pptx files — it produces corrupt output. \
                         Place generated presentations under `artifacts/` or `reports/` unless \
                         the user specifies otherwise."
                .to_owned(),
            input_schema: schema_of::<CreatePresentationInput>(),
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
        let input: CreatePresentationInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `CreatePresentation` must be \
                     {{\"file_path\": \"/abs/path.pptx\", \"title\": \"...\", \
                     \"slides\": [{{\"title\": \"...\", \"bullets\": [...]}}], \
                     \"create_dirs\"?: true}}: {err}."
            ))
        })?;

        let path = require_absolute(&input.file_path, &ctx.cwd)?;

        if ctx.cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }

        ensure_parent(&path, input.create_dirs).await?;

        let slides: Vec<Slide> = input
            .slides
            .into_iter()
            .map(|s| Slide {
                title: s.title,
                bullets: s.bullets,
            })
            .collect();

        let spec = ArtifactBuildSpec::Presentation {
            title: input.title.clone(),
            slides,
        };

        let backend = Arc::clone(&self.backend);
        let bytes = tokio::task::spawn_blocking(move || backend.build(&spec))
            .await
            .map_err(|e| ToolError::Execution(format!("presentation build panicked: {e}")))?
            .map_err(|e| ToolError::Execution(format!("presentation build failed: {e}")))?;

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
                        "Created presentation `{}` ({} bytes).",
                        path.display(),
                        bytes.len(),
                    ))],
                    is_error: false,
                    structured: Some(serde_json::json!({
                        "relativePath": rel,
                        "title": input.title,
                        "kind": "presentation",
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

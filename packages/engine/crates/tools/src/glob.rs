use std::path::{Path, PathBuf};

use async_trait::async_trait;
use globset::GlobBuilder;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::fs::{resolve_search_root, schema_of, truncate_chars};

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 1000;
const MAX_OUTPUT_CHARS: usize = 80_000;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GlobInput {
    pattern: String,
    path: Option<String>,
    max_results: Option<usize>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "Glob".to_owned(),
            description: "Find files matching a glob pattern under `path` (or the session cwd). \
                          Respects gitignore-style ignores through the `ignore` walker. Patterns \
                          without `/` are treated recursively, so `*.rs` behaves like \
                          `**/*.rs`. Use `max_results` to keep output small."
                .to_owned(),
            input_schema: schema_of::<GlobInput>(),
            read_only: true,
            category: ToolCategory::Fs,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: GlobInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `Glob` must be {{\"pattern\": \"**/*.rs\", \"path\": \
                 <optional directory>, \"max_results\": <optional number>}}: {err}."
            ))
        })?;
        if input.pattern.trim().is_empty() {
            return Err(ToolError::InvalidInput(
                "`pattern` cannot be empty. Pass a glob like `**/*.rs`.".to_owned(),
            ));
        }
        let root = resolve_search_root(input.path.as_deref(), &ctx.cwd, "Glob").await?;
        let pattern = normalize_pattern(&input.pattern);
        let limit = input.max_results.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
        let cancel = ctx.cancel.clone();

        let handle = tokio::task::spawn_blocking(move || glob_files(&root, &pattern, limit));
        let mut paths = tokio::select! {
            _ = cancel.cancelled() => return Err(ToolError::Cancelled),
            result = handle => result.map_err(|err| {
                ToolError::Execution(format!("Glob worker failed before producing results: {err}."))
            })??,
        };
        paths.sort();

        let mut rendered = if paths.is_empty() {
            "[no matches]".to_owned()
        } else {
            paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("\n")
        };
        if paths.len() == limit {
            rendered.push_str("\n\n[... results capped by max_results ...]");
        }
        let (rendered, truncated) = truncate_chars(&rendered, MAX_OUTPUT_CHARS);
        Ok(ToolOutput {
            content: vec![agentloop_contracts::ToolResultBlock::markdown(rendered)],
            is_error: false,
            structured: Some(serde_json::json!({
                "matches": paths,
                "truncated": truncated,
            })),
        })
    }
}

fn normalize_pattern(pattern: &str) -> String {
    if pattern.contains('/') || pattern.starts_with("**") {
        pattern.to_owned()
    } else {
        format!("**/{pattern}")
    }
}

fn glob_files(root: &Path, pattern: &str, limit: usize) -> Result<Vec<PathBuf>, ToolError> {
    let glob = GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map_err(|err| {
            ToolError::InvalidInput(format!(
                "`{pattern}` is not a valid glob pattern for Glob: {err}. Fix the pattern and retry."
            ))
        })?;
    let matcher = glob.compile_matcher();
    let mut out = Vec::new();
    for entry in ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        .build()
    {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(path);
        if matcher.is_match(rel) {
            out.push(path.to_path_buf());
            if out.len() >= limit {
                break;
            }
        }
    }
    Ok(out)
}

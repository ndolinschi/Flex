use std::path::Path;

use async_trait::async_trait;
use globset::GlobBuilder;
use regex::RegexBuilder;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::fs::{resolve_search_root, schema_of, truncate_chars};

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 1000;
const MAX_CONTEXT: usize = 5;
const MAX_OUTPUT_CHARS: usize = 100_000;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GrepInput {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
    case_insensitive: Option<bool>,
    context: Option<usize>,
    max_results: Option<usize>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "Grep".to_owned(),
            description: "Search files with a Rust regex. `path` defaults to the session cwd and \
                          `glob` can narrow files (for example `**/*.rs`). Returns \
                          `path:line:content` matches with optional symmetric context. Use \
                          `max_results` to avoid noisy searches."
                .to_owned(),
            input_schema: schema_of::<GrepInput>(),
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
        let input: GrepInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `Grep` must be {{\"pattern\": \"regex\", \"path\": \
                 <optional directory>, \"glob\": <optional glob>, \"case_insensitive\": \
                 <optional bool>, \"context\": <optional lines>, \"max_results\": \
                 <optional number>}}: {err}."
            ))
        })?;
        if input.pattern.trim().is_empty() {
            return Err(ToolError::InvalidInput(
                "`pattern` cannot be empty. Pass a Rust regex like `fn main`.".to_owned(),
            ));
        }
        let root = resolve_search_root(input.path.as_deref(), &ctx.cwd, "Grep").await?;
        let limit = input.max_results.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
        let context = input.context.unwrap_or(0).min(MAX_CONTEXT);
        let case_insensitive = input.case_insensitive.unwrap_or(false);
        let cancel = ctx.cancel.clone();

        let handle = tokio::task::spawn_blocking(move || {
            grep_files(
                &root,
                &input.pattern,
                input.glob.as_deref(),
                case_insensitive,
                context,
                limit,
            )
        });
        let result = tokio::select! {
            _ = cancel.cancelled() => return Err(ToolError::Cancelled),
            result = handle => result.map_err(|err| {
                ToolError::Execution(format!("Grep worker failed before producing results: {err}."))
            })??,
        };

        let mut rendered = if result.lines.is_empty() {
            "[no matches]".to_owned()
        } else {
            result.lines.join("\n")
        };
        if result.capped {
            rendered.push_str("\n\n[... results capped by max_results ...]");
        }
        let (rendered, truncated) = truncate_chars(&rendered, MAX_OUTPUT_CHARS);
        Ok(ToolOutput {
            content: vec![agentloop_contracts::ToolResultBlock::markdown(rendered)],
            is_error: false,
            structured: Some(serde_json::json!({
                "match_count": result.match_count,
                "searched_files": result.searched_files,
                "truncated": truncated || result.capped,
            })),
        })
    }
}

struct GrepResult {
    lines: Vec<String>,
    match_count: usize,
    searched_files: usize,
    capped: bool,
}

fn grep_files(
    root: &Path,
    pattern: &str,
    glob: Option<&str>,
    case_insensitive: bool,
    context: usize,
    limit: usize,
) -> Result<GrepResult, ToolError> {
    let regex = RegexBuilder::new(pattern)
        .case_insensitive(case_insensitive)
        .build()
        .map_err(|err| {
            ToolError::InvalidInput(format!(
                "`{pattern}` is not a valid Rust regex for Grep: {err}. Fix the pattern and retry."
            ))
        })?;
    let matcher = match glob {
        Some(glob) => Some(
            GlobBuilder::new(&normalize_glob(glob))
                .literal_separator(true)
                .build()
                .map_err(|err| {
                    ToolError::InvalidInput(format!(
                        "`{glob}` is not a valid file glob for Grep: {err}. Fix `glob` and retry."
                    ))
                })?
                .compile_matcher(),
        ),
        None => None,
    };

    let mut rendered = Vec::new();
    let mut match_count = 0usize;
    let mut searched_files = 0usize;
    let mut capped = false;

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
        if matcher.as_ref().is_some_and(|m| !m.is_match(rel)) {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(path) else {
            continue;
        };
        searched_files += 1;
        let file_lines: Vec<&str> = contents.lines().collect();
        for (idx, line) in file_lines.iter().enumerate() {
            if !regex.is_match(line) {
                continue;
            }
            match_count += 1;
            push_context(&mut rendered, path, &file_lines, idx, context);
            if match_count >= limit {
                capped = true;
                return Ok(GrepResult {
                    lines: rendered,
                    match_count,
                    searched_files,
                    capped,
                });
            }
        }
    }

    Ok(GrepResult {
        lines: rendered,
        match_count,
        searched_files,
        capped,
    })
}

fn normalize_glob(glob: &str) -> String {
    if glob.contains('/') || glob.starts_with("**") {
        glob.to_owned()
    } else {
        format!("**/{glob}")
    }
}

fn push_context(
    rendered: &mut Vec<String>,
    path: &Path,
    lines: &[&str],
    match_idx: usize,
    context: usize,
) {
    let start = match_idx.saturating_sub(context);
    let end = (match_idx + context + 1).min(lines.len());
    for (idx, line) in lines.iter().enumerate().take(end).skip(start) {
        let marker = if idx == match_idx { ":" } else { "-" };
        rendered.push(format!("{}{}{}:{}", path.display(), marker, idx + 1, line));
    }
}

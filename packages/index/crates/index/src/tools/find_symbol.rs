//! `FindSymbol` tool: exact/prefix lookup of a definition by name (and
//! optional kind) over the repo's extracted symbol table.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{ToolOutput, ToolResultBlock};
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::symbols::{Symbol, SymbolKind};
use crate::tools::shared::{IndexOpenMode, open_and_build_with_events_mode};

const MAX_OUTPUT_CHARS: usize = 8_000;
const MAX_MATCHES: usize = 50;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FindSymbolInput {
    /// Symbol name to find. Exact matches (case-sensitive) rank first,
    /// followed by prefix matches.
    name: String,
    /// Optional kind filter: `function`, `method`, `struct`, `class`,
    /// `const`, `interface`, `enum`, or `heading` (markdown). Omit to match
    /// any kind.
    kind: Option<String>,
}

/// Find where a symbol (function, struct, class, const, method, interface,
/// enum, or markdown heading) is defined.
#[derive(Debug, Clone, Copy)]
pub struct FindSymbolTool {
    open_mode: IndexOpenMode,
}

impl Default for FindSymbolTool {
    fn default() -> Self {
        Self::new(IndexOpenMode::default())
    }
}

impl FindSymbolTool {
    pub fn new(open_mode: IndexOpenMode) -> Self {
        Self { open_mode }
    }
}

#[async_trait]
impl Tool for FindSymbolTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "FindSymbol".to_owned(),
            description: "Find the definition location(s) of a named symbol (function, method, \
                 struct, class, const, interface, enum, or markdown heading). Exact-name matches \
                 rank first, then prefix matches. Set `kind` to narrow to one symbol kind (e.g. \
                 `\"function\"`) when a name is ambiguous. Use this when you know the identifier \
                 you're looking for; use `SearchCode` instead when you only know roughly what \
                 the code does."
                .to_owned(),
            input_schema: schema_of::<FindSymbolInput>(),
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
        let parsed: FindSymbolInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `FindSymbol` must be {{\"name\": \"<symbol name>\", \"kind\": \
                 <optional kind>}}: {err}."
            ))
        })?;

        let name = parsed.name.trim();
        if name.is_empty() {
            return Err(ToolError::InvalidInput(
                "`name` must be a non-empty symbol name for `FindSymbol`.".to_owned(),
            ));
        }
        let kind_filter = match parsed.kind.as_deref() {
            Some(raw) => Some(parse_kind(raw)?),
            None => None,
        };

        let cwd = ctx.cwd.clone();
        let name_owned = name.to_owned();
        let events = ctx.events.clone();
        let call_id = ctx.call_id.clone();
        let cancel = ctx.cancel.clone();
        let open_mode = self.open_mode;
        let handle = tokio::task::spawn_blocking(move || {
            find_matches(&cwd, &name_owned, kind_filter, &events, &call_id, open_mode)
        });
        let matches = tokio::select! {
            _ = cancel.cancelled() => return Err(ToolError::Cancelled),
            result = handle => result.map_err(|err| {
                ToolError::Execution(format!("FindSymbol worker failed before producing results: {err}."))
            })??,
        };

        let rendered = render_matches(name, &matches);
        let (rendered, truncated) = truncate_chars(&rendered, MAX_OUTPUT_CHARS);

        Ok(ToolOutput {
            content: vec![ToolResultBlock::markdown(rendered)],
            is_error: false,
            structured: Some(serde_json::json!({
                "name": name,
                "match_count": matches.len(),
                "truncated": truncated,
                "matches": matches.iter().map(|s| serde_json::json!({
                    "name": s.name,
                    "kind": s.kind,
                    "path": s.path,
                    "start_line": s.start_line,
                    "end_line": s.end_line,
                })).collect::<Vec<_>>(),
            })),
        })
    }
}

fn parse_kind(raw: &str) -> Result<SymbolKind, ToolError> {
    match raw.to_lowercase().as_str() {
        "function" => Ok(SymbolKind::Function),
        "method" => Ok(SymbolKind::Method),
        "struct" => Ok(SymbolKind::Struct),
        "class" => Ok(SymbolKind::Class),
        "const" => Ok(SymbolKind::Const),
        "interface" => Ok(SymbolKind::Interface),
        "enum" => Ok(SymbolKind::Enum),
        "heading" => Ok(SymbolKind::Heading),
        other => Err(ToolError::InvalidInput(format!(
            "`kind` must be one of function, method, struct, class, const, interface, enum, \
             heading, but got `{other}`."
        ))),
    }
}

fn find_matches(
    cwd: &std::path::Path,
    name: &str,
    kind_filter: Option<SymbolKind>,
    events: &agentloop_core::EventSink,
    call_id: &agentloop_contracts::ToolCallId,
    open_mode: IndexOpenMode,
) -> Result<Vec<Symbol>, ToolError> {
    let store = open_and_build_with_events_mode(cwd, events, Some(call_id), open_mode)?;
    Ok(query_matches(&store, name, kind_filter))
}

/// Pure query over an already-open store: exact matches first, then prefix
/// matches, each group path/line sorted, capped at [`MAX_MATCHES`]. Split out
/// from [`find_matches`] so tests can build a store against a scratch index
/// directory (via `shared::open_and_build_in`) instead of depending on
/// `open_and_build`'s real app-data index resolution.
fn query_matches(
    store: &crate::store::IndexStore,
    name: &str,
    kind_filter: Option<SymbolKind>,
) -> Vec<Symbol> {
    let mut exact: Vec<Symbol> = Vec::new();
    let mut prefix: Vec<Symbol> = Vec::new();

    for symbol in store.symbols() {
        if let Some(kind) = kind_filter {
            if symbol.kind != kind {
                continue;
            }
        }
        if symbol.name == name {
            exact.push(symbol.clone());
        } else if symbol.name.starts_with(name) {
            prefix.push(symbol.clone());
        }
    }

    exact.sort_by(|a, b| a.path.cmp(&b.path).then(a.start_line.cmp(&b.start_line)));
    prefix.sort_by(|a, b| a.path.cmp(&b.path).then(a.start_line.cmp(&b.start_line)));

    exact.extend(prefix);
    exact.truncate(MAX_MATCHES);
    exact
}

fn render_matches(name: &str, matches: &[Symbol]) -> String {
    if matches.is_empty() {
        return format!(
            "No symbol named `{name}` (or with that prefix) was found. Try `SearchCode` for a \
             broader natural-language search."
        );
    }
    let mut out = String::new();
    for symbol in matches {
        out.push_str(&format!(
            "{}:{}-{} — {} ({:?})\n",
            symbol.path, symbol.start_line, symbol.end_line, symbol.name, symbol.kind
        ));
    }
    out
}

fn schema_of<I: JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(I))
        .unwrap_or_else(|_| serde_json::json!({"type": "object"}))
}

fn truncate_chars(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_owned(), false);
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("\n\n[... output truncated ...]");
    (out, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(root: &std::path::Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{e}"));
        }
        fs::write(path, content).unwrap_or_else(|e| panic!("{e}"));
    }

    #[test]
    fn find_matches_exact_hit_ranks_before_prefix() {
        let index_root = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        write(
            repo.path(),
            "a.rs",
            "fn connect() {}\nfn connect_timeout() {}\n",
        );

        let store = crate::tools::shared::open_and_build_in(repo.path(), index_root.path())
            .unwrap_or_else(|e| panic!("{e}"));
        let matches = query_matches(&store, "connect", None);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].name, "connect");
    }

    #[test]
    fn find_matches_respects_kind_filter() {
        let index_root = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        write(
            repo.path(),
            "a.rs",
            "struct Widget;\nfn widget_builder() -> Widget { Widget }\n",
        );

        let store = crate::tools::shared::open_and_build_in(repo.path(), index_root.path())
            .unwrap_or_else(|e| panic!("{e}"));
        let matches = query_matches(&store, "Widget", Some(SymbolKind::Struct));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].kind, SymbolKind::Struct);
    }

    #[test]
    fn render_matches_empty() {
        let rendered = render_matches("missing_fn", &[]);
        assert!(rendered.contains("No symbol named"));
    }

    /// M1 live-acceptance: `FindSymbol` `Tool::run` with the index-root
    /// override so tests never write to real Application Support.
    #[tokio::test]
    #[allow(clippy::await_holding_lock)] // gate must cover Tool::run's spawn_blocking
    async fn live_accept_find_symbol_tool_run() {
        use agentloop_contracts::{SessionId, ToolCallId, TurnId};
        use agentloop_core::{EventSink, Tool};
        use tokio_util::sync::CancellationToken;

        use crate::tools::shared::{
            index_dir_for, index_root_base, lock_index_root_override, set_index_root_override,
        };

        let index_root = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        write(
            repo.path(),
            "src/session_title.rs",
            "pub fn generate_session_title(msg: &str) -> String { msg.to_owned() }\n",
        );
        write(repo.path(), "src/other.rs", "pub fn unrelated() {}\n");

        let _gate = lock_index_root_override();
        set_index_root_override(Some(index_root.path().to_path_buf()));
        let live_index = index_dir_for(repo.path(), &index_root_base());

        let (events, _rx) = EventSink::channel();
        let ctx = ToolContext {
            session_id: SessionId::from("sess-live"),
            turn_id: TurnId::from("turn-live"),
            call_id: ToolCallId::from("call-live"),
            cwd: repo.path().to_path_buf(),
            cancel: CancellationToken::new(),
            events,
        };

        let tool = FindSymbolTool::default();
        let output = tool
            .run(
                ctx,
                serde_json::json!({ "name": "generate_session_title", "kind": "function" }),
            )
            .await;
        set_index_root_override(None);
        drop(_gate);
        let output = output.unwrap_or_else(|e| panic!("FindSymbol Tool::run failed: {e}"));
        assert!(!output.is_error);
        let structured = output.structured.expect("structured");
        assert!(
            structured["match_count"].as_u64().unwrap_or(0) >= 1,
            "expected at least one match: {structured}"
        );
        let first = &structured["matches"][0];
        assert_eq!(first["name"], "generate_session_title");
        assert!(
            first["path"]
                .as_str()
                .unwrap_or("")
                .contains("session_title"),
            "path={}",
            first["path"]
        );
        assert!(live_index.exists(), "index at {live_index:?}");
        assert!(!live_index.starts_with(repo.path()));
    }
}

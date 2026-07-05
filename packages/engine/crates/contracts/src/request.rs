//! Inputs to an agent: prompts, per-turn options, session creation params.

use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::capability::ModelRef;
use crate::content::ContentBlock;
use crate::permission::PermissionMode;
use crate::workspace::IsolationPolicy;

/// Metadata for a slash command expanded before a turn starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExpandedCommand {
    /// Command name without the leading slash.
    pub name: String,
    /// Raw argument text after the command name.
    pub args: String,
}

/// What the user submits for one turn: markdown text plus optional image and
/// file attachments.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PromptInput {
    pub parts: Vec<ContentBlock>,
    /// Present when EngineService expanded a recognized slash command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<ExpandedCommand>,
}

impl PromptInput {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            parts: vec![ContentBlock::markdown(text)],
            command: None,
        }
    }

    /// The concatenated markdown text of the prompt (attachments excluded).
    pub fn joined_text(&self) -> String {
        let mut out = String::new();
        for part in &self.parts {
            if let ContentBlock::Markdown { text } = part {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(text);
            }
        }
        out
    }
}

/// Canonical extended-thinking configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkingConfig {
    pub budget_tokens: u32,
}

/// How hard the agent should work on a turn. Maps (in the loop) to an
/// extended-thinking budget scaled per provider and per role, to
/// reasoning-guidance prompt text, and — at the top tiers — to a more
/// aggressive orchestration posture. Ordered low → high.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    /// Favor speed: minimal reasoning, terse, fewest tool calls.
    Low,
    /// Balanced speed and rigor.
    Medium,
    /// Think before acting, read touched code, verify. The default.
    #[default]
    High,
    /// Long-horizon or tricky work: broad exploration, edge cases,
    /// decompose across parallel subagents, mandatory reviewer pass.
    XHigh,
    /// Correctness dominates cost: exhaustive exploration and cross-verification.
    Max,
}

/// Per-turn options.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TurnOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    /// Models to fall back to (in order) when the effective model's provider
    /// fails mid-turn (rate limit, outage). The conversation continues on the
    /// next chain entry; partial output from the failed attempt is discarded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_models: Vec<ModelRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<PermissionMode>,
    /// Extra system-prompt text appended for this turn only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_append: Option<String>,
    /// Extended-thinking budget for this turn; `None` = provider default
    /// (off). Forwarded only to providers that declare the thinking
    /// capability. When set, an explicit budget here wins over the one the
    /// loop would derive from [`Self::effort`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// How hard to work this turn: scales the thinking budget, the
    /// reasoning-guidance prompt text, and (at xhigh/max) the orchestration
    /// posture. `None` leaves behavior unchanged (engine default); the CLI
    /// always sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Namespaced passthrough for agent-specific options.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Options for creating a session.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NewSessionParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Working directory; defaults to the engine process's cwd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<PermissionMode>,
    /// Requested isolation posture for this session. `None` defers to the
    /// engine/role default. Only honored for root sessions; subagents inherit
    /// their parent's working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation: Option<IsolationPolicy>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

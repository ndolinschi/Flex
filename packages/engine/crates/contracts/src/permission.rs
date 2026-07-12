//! Permission and user-question wire types.
//!
//! Rule *matching semantics* (glob paths, command prefixes) are policy logic
//! and live in the loop crate; this module owns the data shapes and the
//! `ToolName(specifier)` rule syntax.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How the engine treats tool calls that would normally ask the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionMode {
    /// Ask on anything not covered by a rule.
    Default,
    /// Auto-allow file edits (the `Fs` category); ask for the rest.
    AcceptEdits,
    /// Deny all mutating tools; read-only research still runs.
    Plan,
    /// Never ask: deny anything that would prompt.
    DontAsk,
    /// Allow everything. Requires an explicit opt-in flag on the runner.
    BypassPermissions,
}

/// A user's answer to a permission request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "decision", rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionDecision {
    AllowOnce,
    /// Allow and persist a rule so this is never asked again.
    AllowAlways,
    Deny {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

/// The options a client may present for a pending permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionDecisionKind {
    AllowOnce,
    AllowAlways,
    Deny,
}

/// Whether a matching [`PermissionRule`] grants or refuses the call.
///
/// Rules are evaluated **last-match-wins**, so a later `Deny` overrides an
/// earlier `Allow` and vice versa. The default is `Allow`, so a plain rule
/// string (no `!` prefix) is an allow rule — every pre-existing rule keeps its
/// meaning.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RuleEffect {
    #[default]
    Allow,
    Deny,
}

/// A permission rule in the `ToolName(specifier)` format:
/// `Bash(git *)`, `Read(~/secrets/**)`, `WebFetch(domain:example.com)`,
/// or a bare tool name (`WebSearch`). A leading `!` makes it a **deny** rule
/// (gitignore style): `!Edit(**/.env*)` refuses edits to dotenv files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct PermissionRule {
    pub tool: String,
    pub specifier: Option<String>,
    /// Allow (default) or Deny. Encoded on the wire by a leading `!`.
    #[serde(default)]
    pub effect: RuleEffect,
}

impl PermissionRule {
    /// Parse `Tool(spec)` / `Tool`, with an optional leading `!` for a deny
    /// rule. Returns `None` for malformed input (empty tool name, unbalanced
    /// parentheses).
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        let (effect, raw) = match raw.strip_prefix('!') {
            Some(rest) => (RuleEffect::Deny, rest.trim_start()),
            None => (RuleEffect::Allow, raw),
        };
        if raw.is_empty() {
            return None;
        }
        match raw.split_once('(') {
            None => {
                if raw.contains(')') {
                    None
                } else {
                    Some(Self {
                        tool: raw.to_owned(),
                        specifier: None,
                        effect,
                    })
                }
            }
            Some((tool, rest)) => {
                let tool = tool.trim();
                let spec = rest.strip_suffix(')')?;
                if tool.is_empty() {
                    return None;
                }
                Some(Self {
                    tool: tool.to_owned(),
                    specifier: Some(spec.to_owned()),
                    effect,
                })
            }
        }
    }
}

impl fmt::Display for PermissionRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.effect == RuleEffect::Deny {
            f.write_str("!")?;
        }
        match &self.specifier {
            Some(spec) => write!(f, "{}({})", self.tool, spec),
            None => f.write_str(&self.tool),
        }
    }
}

impl TryFrom<String> for PermissionRule {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value).ok_or_else(|| format!("malformed permission rule: {value:?}"))
    }
}

impl From<PermissionRule> for String {
    fn from(rule: PermissionRule) -> Self {
        rule.to_string()
    }
}

/// One option of a multiple-choice question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

const fn default_allow_custom() -> bool {
    true
}

/// A structured question the agent asks the user (`AskUserQuestion` tool).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Question {
    /// Short chip/tag label (e.g. "Auth method").
    pub header: String,
    /// The full question text.
    pub question: String,
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multi_select: bool,
    /// When true the user may type a free-text answer instead of picking options.
    #[serde(default = "default_allow_custom")]
    pub allow_custom: bool,
}

/// The user's answer to one [`Question`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Answer {
    /// The question text being answered.
    pub question: String,
    /// Selected option labels (one unless `multi_select`), or free text.
    pub selected: Vec<String>,
}

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    Plan,
    DontAsk,
    BypassPermissions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "decision", rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionDecision {
    AllowOnce,
    AllowAlways,
    Deny {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionDecisionKind {
    AllowOnce,
    AllowAlways,
    Deny,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RuleEffect {
    #[default]
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct PermissionRule {
    pub tool: String,
    pub specifier: Option<String>,
    #[serde(default)]
    pub effect: RuleEffect,
}

impl PermissionRule {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

const fn default_allow_custom() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Question {
    pub header: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multi_select: bool,
    #[serde(default = "default_allow_custom")]
    pub allow_custom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Answer {
    pub question: String,
    pub selected: Vec<String>,
}

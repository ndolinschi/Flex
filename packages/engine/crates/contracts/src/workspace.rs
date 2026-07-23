use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum IsolationPolicy {
    #[default]
    Never,
    Optional,
    Required,
}

impl IsolationPolicy {
    pub fn wants_isolation(self) -> bool {
        matches!(self, Self::Optional | Self::Required)
    }

    pub fn is_required(self) -> bool {
        matches!(self, Self::Required)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
#[non_exhaustive]
pub enum IntegrationOutcome {
    Merged { files_changed: u32 },
    VerifyFailed { detail: String },
    Diverged { branch: String },
    Empty,
}

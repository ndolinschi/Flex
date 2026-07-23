use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum BackgroundAction {
    Status,
    Kill,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct BashInput {
    pub(super) command: Option<String>,
    pub(super) timeout_ms: Option<u64>,
    #[serde(default)]
    pub(super) run_in_background: bool,
    pub(super) background_action: Option<BackgroundAction>,
    pub(super) process_id: Option<String>,
}


use super::common::require_service;
use super::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfoDto {
    pub name: String,
    pub description: String,
    pub args_hint: Option<String>,
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn list_commands(state: State<'_, AppState>) -> DesktopResult<Vec<CommandInfoDto>> {
    let service = require_service(&state).await?;
    let hello = service.hello();
    Ok(hello
        .capabilities
        .commands
        .into_iter()
        .map(|c: CommandInfo| CommandInfoDto {
            name: c.name,
            description: c.description,
            args_hint: c.args_hint,
        })
        .collect())
}

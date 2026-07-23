
use super::common::require_service;
use super::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineTriggerDto {
    pub kind: String,
    pub expr: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineDto {
    pub id: String,
    pub prompt: String,
    pub max_iterations: u32,
    pub max_identical_failures: u32,
    pub token_budget: Option<u64>,
    pub require_verification: bool,
    pub trigger: RoutineTriggerDto,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineRunRecordDto {
    pub session_id: String,
    pub started_ms: u64,
    pub stop_reason: String,
    pub iterations: u32,
}

pub(crate) fn routine_trigger_from_dto(dto: &RoutineTriggerDto) -> DesktopResult<RoutineTrigger> {
    match dto.kind.as_str() {
        "cron" => {
            let expr = dto
                .expr
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| DesktopError::Message("cron trigger requires `expr`".into()))?;
            Ok(RoutineTrigger::Cron {
                expr: expr.to_owned(),
            })
        }
        "webhook" => {
            let path = dto
                .path
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| DesktopError::Message("webhook trigger requires `path`".into()))?;
            Ok(RoutineTrigger::Webhook {
                path: path.to_owned(),
            })
        }
        other => Err(DesktopError::Message(format!(
            "unknown routine trigger kind: {other}"
        ))),
    }
}

pub(crate) fn routine_trigger_to_dto(trigger: &RoutineTrigger) -> RoutineTriggerDto {
    match trigger {
        RoutineTrigger::Cron { expr } => RoutineTriggerDto {
            kind: "cron".into(),
            expr: Some(expr.clone()),
            path: None,
        },
        RoutineTrigger::Webhook { path } => RoutineTriggerDto {
            kind: "webhook".into(),
            expr: None,
            path: Some(path.clone()),
        },
    }
}

pub(crate) fn routine_dto_to_spec(dto: RoutineDto) -> DesktopResult<RoutineSpec> {
    let trigger = routine_trigger_from_dto(&dto.trigger)?;
    Ok(RoutineSpec {
        id: dto.id,
        goal: GoalSpec {
            prompt: dto.prompt,
            max_iterations: dto.max_iterations,
            max_identical_failures: dto.max_identical_failures,
            token_budget: dto.token_budget,
            require_verification: dto.require_verification,
        },
        session_seed: NewSessionParams {
            title: dto.title,
            cwd: dto.cwd.map(PathBuf::from),
            model: dto.model.map(ModelRef),
            ..NewSessionParams::default()
        },
        trigger,
    })
}

pub(crate) fn routine_spec_to_dto(spec: RoutineSpec) -> RoutineDto {
    RoutineDto {
        id: spec.id,
        prompt: spec.goal.prompt,
        max_iterations: spec.goal.max_iterations,
        max_identical_failures: spec.goal.max_identical_failures,
        token_budget: spec.goal.token_budget,
        require_verification: spec.goal.require_verification,
        trigger: routine_trigger_to_dto(&spec.trigger),
        title: spec.session_seed.title,
        cwd: spec
            .session_seed
            .cwd
            .map(|p| p.to_string_lossy().into_owned()),
        model: spec.session_seed.model.map(|m| m.0),
    }
}

pub(crate) fn validate_routine_id(id: &str) -> DesktopResult<&str> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("routine id is required".into()));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.chars().any(char::is_whitespace) {
        return Err(DesktopError::Message(
            "routine id must not contain slashes or whitespace".into(),
        ));
    }
    Ok(trimmed)
}

pub(crate) fn routine_store() -> DesktopResult<FileRoutineStore> {
    FileRoutineStore::with_default_dir()
        .ok_or_else(|| DesktopError::Message("could not resolve home directory".into()))
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn routines_list() -> DesktopResult<Vec<RoutineDto>> {
    let store = routine_store()?;
    let mut specs = RoutineStore::list(&store)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;
    specs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(specs.into_iter().map(routine_spec_to_dto).collect())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn routines_upsert(routine: RoutineDto) -> DesktopResult<()> {
    validate_routine_id(&routine.id)?;
    if routine.prompt.trim().is_empty() {
        return Err(DesktopError::Message("routine prompt is required".into()));
    }
    let spec = routine_dto_to_spec(routine)?;
    let store = routine_store()?;
    RoutineStore::upsert(&store, spec)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn routines_remove(id: String) -> DesktopResult<()> {
    let id = validate_routine_id(&id)?;
    let store = routine_store()?;
    RoutineStore::remove(&store, id)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn routines_run(state: State<'_, AppState>, id: String) -> DesktopResult<()> {
    validate_routine_id(&id)?;
    let service = require_service(&state).await?;
    let store = routine_store()?;
    let runner = RoutineRunner::new(Arc::new(service), Arc::new(store));
    tauri::async_runtime::spawn(async move {
        if let Err(e) = runner.run_by_id(&id).await {
            tracing::warn!(error = %e, routine = %id, "routine run failed");
        }
    });
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn routines_history(id: String) -> DesktopResult<Vec<RoutineRunRecordDto>> {
    let id = validate_routine_id(&id)?;
    let Some(dir) = default_routines_dir() else {
        return Err(DesktopError::Message(
            "could not resolve home directory".into(),
        ));
    };
    let path = dir.join(format!("{id}.history.jsonl"));
    let content = match tokio::fs::read_to_string(&path).await {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(DesktopError::Message(e.to_string())),
    };

    let mut records = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<agentloop_channel::RoutineRunRecord>(line) {
            Ok(record) => {
                let stop_reason = serde_json::to_value(record.outcome.stop_reason)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "unknown".into());
                records.push(RoutineRunRecordDto {
                    session_id: record.session_id.as_str().to_owned(),
                    started_ms: record.started_ms,
                    stop_reason,
                    iterations: record.outcome.iterations,
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, routine = %id, "skipping malformed routine history line");
            }
        }
    }
    Ok(records)
}

pub async fn respawn_cron_loop(state: &AppState) {
    if let Some(token) = state.routine_cancel.lock().await.take() {
        token.cancel();
    }

    let service = match require_service(state).await {
        Ok(service) => service,
        Err(_) => return,
    };
    let store = match routine_store() {
        Ok(store) => store,
        Err(e) => {
            tracing::warn!(error = %e, "routines store unavailable, cron loop not started");
            return;
        }
    };

    let runner = Arc::new(RoutineRunner::new(Arc::new(service), Arc::new(store)));
    let token = CancellationToken::new();
    tauri::async_runtime::spawn(runner.spawn_cron_loop(token.clone()));
    *state.routine_cancel.lock().await = Some(token);
}

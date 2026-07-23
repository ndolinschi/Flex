use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Router, middleware};
use chrono::{DateTime, Utc};
use croner::Cron;
use tokio_util::sync::CancellationToken;

use agentloop_channel::{
    RoutineError, RoutineRunRecord, RoutineSpec, RoutineStore, RoutineTrigger,
};
use agentloop_contracts::{GoalOutcome, now_ms};
use agentloop_engine::{EngineService, EngineServiceError};
use agentloop_transport_http::{AuthToken, require_bearer_token};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RoutineRunError {
    #[error("routine `{0}` not found")]
    NotFound(String),
    #[error("{0}")]
    Store(RoutineError),
    #[error(transparent)]
    Engine(#[from] EngineServiceError),
}

pub struct RoutineRunner {
    engine: Arc<EngineService>,
    store: Arc<dyn RoutineStore>,
}

const CRON_POLL_INTERVAL: Duration = Duration::from_secs(30);

impl RoutineRunner {
    pub fn new(engine: Arc<EngineService>, store: Arc<dyn RoutineStore>) -> Self {
        Self { engine, store }
    }

    pub fn store(&self) -> &Arc<dyn RoutineStore> {
        &self.store
    }

    pub async fn run_once(&self, spec: &RoutineSpec) -> Result<GoalOutcome, EngineServiceError> {
        let session = self
            .engine
            .create_session(spec.session_seed.clone())
            .await?;
        let started_ms = now_ms();
        let outcome = self.engine.run_goal(&session, spec.goal.clone()).await?;
        let record = RoutineRunRecord {
            session_id: session,
            started_ms,
            outcome: outcome.clone(),
        };
        if let Err(err) = self.store.record_run(&spec.id, record).await {
            tracing::warn!(
                target: "routines",
                routine = %spec.id,
                "run succeeded but its history could not be recorded: {err}"
            );
        }
        Ok(outcome)
    }

    pub async fn run_by_id(&self, id: &str) -> Result<GoalOutcome, RoutineRunError> {
        let spec = self
            .store
            .get(id)
            .await
            .map_err(RoutineRunError::Store)?
            .ok_or_else(|| RoutineRunError::NotFound(id.to_owned()))?;
        Ok(self.run_once(&spec).await?)
    }

    pub async fn spawn_cron_loop(self: Arc<Self>, cancel: CancellationToken) {
        let mut last_tick = Utc::now();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(CRON_POLL_INTERVAL) => {}
            }
            let now = Utc::now();
            self.run_due_cron_routines(last_tick, now).await;
            last_tick = now;
        }
    }

    async fn run_due_cron_routines(self: &Arc<Self>, since: DateTime<Utc>, now: DateTime<Utc>) {
        let specs = match self.store.list().await {
            Ok(specs) => specs,
            Err(err) => {
                tracing::warn!(target: "routines", "could not list routines: {err}");
                return;
            }
        };
        for spec in specs {
            let RoutineTrigger::Cron { expr } = &spec.trigger else {
                continue;
            };
            let Some(due) = cron_is_due(expr, since, now) else {
                tracing::warn!(
                    target: "routines",
                    routine = %spec.id,
                    "invalid cron expression `{expr}`"
                );
                continue;
            };
            if !due {
                continue;
            }
            let runner = Arc::clone(self);
            tokio::spawn(async move {
                if let Err(err) = runner.run_once(&spec).await {
                    tracing::warn!(target: "routines", routine = %spec.id, "run failed: {err}");
                }
            });
        }
    }
}

fn cron_is_due(expr: &str, since: DateTime<Utc>, now: DateTime<Utc>) -> Option<bool> {
    let cron: Cron = expr.parse().ok()?;
    Some(matches!(cron.find_next_occurrence(&since, false), Ok(next) if next <= now))
}

pub fn routine_webhook_router(runner: Arc<RoutineRunner>, token: AuthToken) -> Router {
    Router::new()
        .route("/routines/{id}/trigger", post(trigger_webhook))
        .layer(middleware::from_fn_with_state(token, require_bearer_token))
        .with_state(runner)
}

async fn trigger_webhook(
    State(runner): State<Arc<RoutineRunner>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    match runner.store().get(&id).await {
        Ok(Some(_)) => {}
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
    tokio::spawn(async move {
        if let Err(err) = runner.run_by_id(&id).await {
            tracing::warn!(
                target: "routines",
                routine = %id,
                "webhook-triggered run failed: {err}"
            );
        }
    });
    Ok(StatusCode::ACCEPTED)
}

pub fn default_routines_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("agentloop")
            .join("routines")
    })
}

pub struct FileRoutineStore {
    dir: PathBuf,
}

impl FileRoutineStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn with_default_dir() -> Option<Self> {
        default_routines_dir().map(Self::new)
    }

    fn spec_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.toml"))
    }

    fn history_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.history.jsonl"))
    }
}

fn io_err(err: impl std::fmt::Display) -> RoutineError {
    RoutineError::Storage(err.to_string())
}

#[async_trait]
impl RoutineStore for FileRoutineStore {
    async fn list(&self) -> Result<Vec<RoutineSpec>, RoutineError> {
        let dir = self.dir.clone();
        tokio::task::spawn_blocking(move || {
            let entries = match std::fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
                Err(err) => return Err(io_err(err)),
            };
            let mut specs = Vec::new();
            for entry in entries {
                let path = entry.map_err(io_err)?.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                    continue;
                }
                let content = std::fs::read_to_string(&path).map_err(io_err)?;
                specs.push(toml::from_str(&content).map_err(io_err)?);
            }
            Ok(specs)
        })
        .await
        .map_err(io_err)?
    }

    async fn get(&self, id: &str) -> Result<Option<RoutineSpec>, RoutineError> {
        let path = self.spec_path(id);
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => Ok(Some(toml::from_str(&content).map_err(io_err)?)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(io_err(err)),
        }
    }

    async fn upsert(&self, spec: RoutineSpec) -> Result<(), RoutineError> {
        tokio::fs::create_dir_all(&self.dir).await.map_err(io_err)?;
        let content = toml::to_string_pretty(&spec).map_err(io_err)?;
        tokio::fs::write(self.spec_path(&spec.id), content)
            .await
            .map_err(io_err)
    }

    async fn remove(&self, id: &str) -> Result<(), RoutineError> {
        match tokio::fs::remove_file(self.spec_path(id)).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Err(RoutineError::NotFound(id.to_owned()))
            }
            Err(err) => Err(io_err(err)),
        }
    }

    async fn record_run(&self, id: &str, record: RoutineRunRecord) -> Result<(), RoutineError> {
        let mut line = serde_json::to_string(&record).map_err(io_err)?;
        line.push('\n');
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.history_path(id))
            .await
            .map_err(io_err)?;
        file.write_all(line.as_bytes()).await.map_err(io_err)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration as ChronoDuration;

    use agentloop_contracts::{GoalSpec, ModelRef, NewSessionParams};
    use agentloop_core::ProviderRegistry;
    use agentloop_engine::EngineConfig;
    use agentloop_testkit::{MOCK_MODEL, MOCK_PROVIDER_ID, MockProvider};

    use super::*;

    #[test]
    fn cron_is_due_fires_within_the_since_now_window() {
        let now = Utc::now();
        let a_bit_ago = now - ChronoDuration::minutes(2);
        assert_eq!(
            cron_is_due("* * * * *", a_bit_ago, now),
            Some(true),
            "every-minute cron should have fired at least once in a 2-minute window"
        );
    }

    #[test]
    fn cron_is_due_does_not_fire_on_an_empty_window() {
        let now = Utc::now();

        assert_eq!(cron_is_due("0 0 1 1 *", now, now), Some(false));
    }

    #[test]
    fn cron_is_due_is_none_for_an_invalid_expression() {
        assert_eq!(
            cron_is_due("not a cron expression", Utc::now(), Utc::now()),
            None
        );
    }

    fn sample_spec(id: &str) -> RoutineSpec {
        RoutineSpec {
            id: id.to_owned(),
            goal: GoalSpec {
                prompt: "say hello".to_owned(),
                max_iterations: 3,
                max_identical_failures: 3,
                token_budget: None,
                require_verification: false,
            },
            session_seed: NewSessionParams::default(),
            trigger: RoutineTrigger::Cron {
                expr: "0 * * * *".to_owned(),
            },
        }
    }

    #[tokio::test]
    async fn file_store_round_trips_a_routine() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileRoutineStore::new(dir.path());

        assert!(store.list().await.expect("list").is_empty());
        assert!(store.get("r1").await.expect("get").is_none());

        store.upsert(sample_spec("r1")).await.expect("upsert");
        let fetched = store.get("r1").await.expect("get").expect("present");
        assert_eq!(fetched.id, "r1");
        assert_eq!(store.list().await.expect("list").len(), 1);

        store.remove("r1").await.expect("remove");
        assert!(store.get("r1").await.expect("get").is_none());
        assert!(matches!(
            store.remove("r1").await,
            Err(RoutineError::NotFound(id)) if id == "r1"
        ));
    }

    fn default_model() -> ModelRef {
        ModelRef(format!("{MOCK_PROVIDER_ID}/{MOCK_MODEL}"))
    }

    #[tokio::test]
    async fn run_once_opens_a_session_and_records_history() {
        let provider = Arc::new(MockProvider::with_turns([MockProvider::text_turn(
            "all done, nothing left to do",
        )]));
        let mut providers = ProviderRegistry::new();
        providers.register(provider);
        let engine = Arc::new(
            EngineService::native(providers, Some(default_model()), EngineConfig::default())
                .expect("engine builds"),
        );
        let dir = tempfile::tempdir().expect("tempdir");
        let store: Arc<dyn RoutineStore> = Arc::new(FileRoutineStore::new(dir.path()));
        let runner = RoutineRunner::new(engine, store.clone());

        let spec = sample_spec("r1");
        let outcome = runner.run_once(&spec).await.expect("run succeeds");
        assert_eq!(
            outcome.stop_reason,
            agentloop_contracts::GoalStopReason::Achieved
        );

        let history = dir.path().join("r1.history.jsonl");
        let content = tokio::fs::read_to_string(&history)
            .await
            .expect("history file was written");
        assert!(content.contains("\"session_id\""));
    }

    #[tokio::test]
    async fn run_by_id_reports_not_found_for_an_unknown_routine() {
        let provider = Arc::new(MockProvider::with_turns([MockProvider::text_turn("done")]));
        let mut providers = ProviderRegistry::new();
        providers.register(provider);
        let engine = Arc::new(
            EngineService::native(providers, Some(default_model()), EngineConfig::default())
                .expect("engine builds"),
        );
        let dir = tempfile::tempdir().expect("tempdir");
        let store: Arc<dyn RoutineStore> = Arc::new(FileRoutineStore::new(dir.path()));
        let runner = RoutineRunner::new(engine, store);

        assert!(matches!(
            runner.run_by_id("missing").await,
            Err(RoutineRunError::NotFound(id)) if id == "missing"
        ));
    }
}

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EvalError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid task file {path}: {message}")]
    Task { path: PathBuf, message: String },

    #[error("no tasks matched under {0}")]
    NoTasks(PathBuf),

    #[error("failed to build service: {0}")]
    Service(String),

    #[error(transparent)]
    Engine(#[from] agentloop_engine::EngineServiceError),

    #[error("json serialization failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid baseline report {path}: {message}")]
    Baseline { path: PathBuf, message: String },
}

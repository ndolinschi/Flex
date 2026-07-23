use std::collections::BTreeMap;
mod line_agent;
mod stream_host;
mod tokio_host;

pub use line_agent::{DelegatedSessionHandle, DelegatorProfile, LineDelegatorAgent};
pub use stream_host::{DuplexProcess, StreamHost, TokioStreamHost};
pub use tokio_host::TokioCommandHost;

use std::path::PathBuf;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{ToolCallId, ToolOutput, TurnStopReason};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegatorProcessSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
}

impl DelegatorProcessSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegatorRunRequest {
    pub spec: DelegatorProcessSpec,
    pub stdin: Option<String>,
}

impl DelegatorRunRequest {
    pub fn new(spec: DelegatorProcessSpec) -> Self {
        Self { spec, stdin: None }
    }

    pub fn stdin(mut self, stdin: impl Into<String>) -> Self {
        self.stdin = Some(stdin.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegatorExitStatus {
    pub code: Option<i32>,
    pub success: bool,
}

impl DelegatorExitStatus {
    pub fn success() -> Self {
        Self {
            code: Some(0),
            success: true,
        }
    }

    pub fn failure(code: Option<i32>) -> Self {
        Self {
            code,
            success: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegatorRunOutput {
    pub stdout_lines: Vec<String>,
    pub stderr: String,
    pub status: DelegatorExitStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegatorProbeStatus {
    Installed { version: Option<String> },
    NotInstalled { hint: String },
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DelegatorHostError {
    #[error("delegator command `{program}` was not found: {hint}")]
    NotInstalled { program: String, hint: String },
    #[error("delegator process I/O failure: {0}")]
    Io(String),
    #[error("delegator process output was not UTF-8: {0}")]
    Utf8(String),
    #[error("delegator process was cancelled")]
    Cancelled,
}

#[async_trait]
pub trait ProcessHost: Send + Sync {
    async fn probe(
        &self,
        spec: &DelegatorProcessSpec,
        cancel: CancellationToken,
    ) -> Result<DelegatorProbeStatus, DelegatorHostError>;

    async fn run(
        &self,
        request: DelegatorRunRequest,
        cancel: CancellationToken,
    ) -> Result<DelegatorRunOutput, DelegatorHostError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum DelegatorEvent {
    AssistantDelta {
        text: String,
    },
    ToolCall {
        call_id: ToolCallId,
        name: String,
        args: serde_json::Value,
    },
    ToolResult {
        call_id: ToolCallId,
        output: ToolOutput,
    },

    Usage {
        usage: agentloop_contracts::TokenUsage,
        cost_usd: Option<f64>,
    },
    TurnFinished {
        stop_reason: TurnStopReason,
    },
    Error {
        message: String,
    },
    Unknown {
        kind: String,
    },
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DelegatorMapError {
    #[error("delegator output was not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("delegator output was missing required field `{0}`")]
    MissingField(&'static str),
}

pub trait LineMapper {
    fn map_line(&mut self, line: &str) -> Result<Vec<DelegatorEvent>, DelegatorMapError>;
}

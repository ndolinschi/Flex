use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::{Mutex, mpsc, oneshot};

use agentloop_delegator_common::{DelegatorHostError, DuplexProcess};

use crate::protocol::AcpNotification;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AcpPermissionPolicy {
    #[default]
    AllowAlways,

    DenyAlways,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AcpClientError {
    #[error("ACP transport failure: {0}")]
    Transport(String),
    #[error("ACP agent returned an error for `{method}`: {message}")]
    Rpc { method: String, message: String },
    #[error("ACP agent closed the connection")]
    Closed,
}

impl From<DelegatorHostError> for AcpClientError {
    fn from(err: DelegatorHostError) -> Self {
        Self::Transport(err.to_string())
    }
}

type PendingResponses =
    Arc<Mutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value, AcpClientError>>>>>;

pub struct AcpClient {
    proc: Arc<DuplexProcess>,
    next_id: AtomicU64,
    pending: PendingResponses,
    updates_tx: mpsc::Sender<AcpNotification>,
}

impl AcpClient {
    pub fn start(
        proc: DuplexProcess,
        policy: AcpPermissionPolicy,
    ) -> (Arc<Self>, mpsc::Receiver<AcpNotification>) {
        let (updates_tx, updates_rx) = mpsc::channel(256);
        let client = Arc::new(Self {
            proc: Arc::new(proc),
            next_id: AtomicU64::new(1),
            pending: Arc::new(Mutex::new(HashMap::new())),
            updates_tx,
        });
        let reader = client.clone();
        tokio::spawn(async move { reader.read_loop(policy).await });
        (client, updates_rx)
    }

    pub async fn request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, AcpClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let frame = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let line = serde_json::to_string(&frame)
            .map_err(|err| AcpClientError::Transport(err.to_string()))?;
        if let Err(err) = self.proc.send_line(line).await {
            self.pending.lock().await.remove(&id);
            return Err(err.into());
        }
        rx.await.map_err(|_| AcpClientError::Closed)?
    }

    pub async fn notify(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), AcpClientError> {
        let frame = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let line = serde_json::to_string(&frame)
            .map_err(|err| AcpClientError::Transport(err.to_string()))?;
        self.proc.send_line(line).await.map_err(Into::into)
    }

    async fn read_loop(&self, policy: AcpPermissionPolicy) {
        while let Some(line) = self.proc.next_line().await {
            let Ok(frame) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let has_id = frame.get("id").is_some();
            let has_method = frame.get("method").is_some();
            match (has_id, has_method) {
                (true, false) => {
                    let Some(id) = frame.get("id").and_then(serde_json::Value::as_u64) else {
                        continue;
                    };
                    let outcome = if let Some(error) = frame.get("error") {
                        Err(AcpClientError::Rpc {
                            method: "request".to_owned(),
                            message: error
                                .get("message")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("unknown error")
                                .to_owned(),
                        })
                    } else {
                        Ok(frame
                            .get("result")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null))
                    };
                    if let Some(tx) = self.pending.lock().await.remove(&id) {
                        let _ = tx.send(outcome);
                    }
                }

                (false, true) => {
                    if let Ok(notification) = serde_json::from_value::<AcpNotification>(frame) {
                        if self.updates_tx.send(notification).await.is_err() {
                            break;
                        }
                    }
                }

                (true, true) => {
                    let id = frame.get("id").cloned().unwrap_or(serde_json::Value::Null);
                    let method = frame
                        .get("method")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    let response = self.answer_agent_request(&method, &frame, policy);
                    let mut reply = serde_json::json!({ "jsonrpc": "2.0", "id": id });
                    match response {
                        Ok(result) => {
                            reply["result"] = result;
                        }
                        Err((code, message)) => {
                            reply["error"] = serde_json::json!({
                                "code": code,
                                "message": message,
                            });
                        }
                    }
                    if let Ok(line) = serde_json::to_string(&reply) {
                        if self.proc.send_line(line).await.is_err() {
                            break;
                        }
                    }
                }
                _ => {}
            }
        }

        let mut pending = self.pending.lock().await;
        for (_, tx) in pending.drain() {
            let _ = tx.send(Err(AcpClientError::Closed));
        }
    }

    fn answer_agent_request(
        &self,
        method: &str,
        frame: &serde_json::Value,
        policy: AcpPermissionPolicy,
    ) -> Result<serde_json::Value, (i64, String)> {
        match method {
            "session/request_permission" => {
                let options = frame
                    .pointer("/params/options")
                    .and_then(serde_json::Value::as_array);
                match policy {
                    AcpPermissionPolicy::DenyAlways => Ok(serde_json::json!({
                        "outcome": { "outcome": "cancelled" }
                    })),
                    AcpPermissionPolicy::AllowAlways => {
                        let option_id = options
                            .and_then(|options| {
                                options
                                    .iter()
                                    .find(|option| {
                                        option
                                            .get("kind")
                                            .and_then(serde_json::Value::as_str)
                                            .is_some_and(|kind| kind.starts_with("allow"))
                                    })
                                    .or_else(|| options.first())
                            })
                            .and_then(|option| option.get("optionId").or(option.get("id")))
                            .cloned()
                            .unwrap_or(serde_json::Value::String("allow".to_owned()));
                        Ok(serde_json::json!({
                            "outcome": { "outcome": "selected", "optionId": option_id }
                        }))
                    }
                }
            }
            other => Err((
                -32601,
                format!("client method `{other}` is not supported by this ACP client (v1)"),
            )),
        }
    }
}

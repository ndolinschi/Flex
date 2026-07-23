use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use rmcp::RoleClient;
use rmcp::model::CallToolResult;
use rmcp::model::{CallToolRequestParams, ContentBlock, Tool as RmcpTool};
use rmcp::service::{ClientInitializeError, RunningService, ServiceError, serve_client_with_ct};
use rmcp::transport::{
    ConfigureCommandExt, StreamableHttpClientTransport, TokioChildProcess,
    streamable_http_client::StreamableHttpClientTransportConfig,
};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use agentloop_contracts::ToolOutput;

use crate::bridge::{
    McpBridgeError, McpContent, McpRemoteTool, McpToolCall, McpToolClient, McpToolResult,
};
use crate::config::{McpServerConfig, McpServerTransport};

type ClientSession = RunningService<RoleClient, ()>;

struct ServerSession {
    service: Mutex<Option<ClientSession>>,
}

pub struct RmcpToolClient {
    servers: BTreeMap<String, McpServerConfig>,
    sessions: RwLock<BTreeMap<String, Arc<ServerSession>>>,
    shutdown: CancellationToken,
}

impl RmcpToolClient {
    pub fn new(servers: impl IntoIterator<Item = McpServerConfig>) -> Self {
        Self {
            servers: servers
                .into_iter()
                .map(|server| (server.name.clone(), server))
                .collect(),
            sessions: RwLock::new(BTreeMap::new()),
            shutdown: CancellationToken::new(),
        }
    }

    pub fn from_configs(configs: &[McpServerConfig]) -> Self {
        Self::new(configs.iter().cloned())
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    pub async fn close_connections(&self) {
        self.shutdown.cancel();
        let sessions = {
            let mut guard = self.sessions.write().await;
            std::mem::take(&mut *guard)
        };
        for (_, session) in sessions {
            let mut guard = session.service.lock().await;
            if let Some(mut service) = guard.take() {
                let _ = service.close().await;
            }
        }
    }

    fn server_config(&self, name: &str) -> Result<&McpServerConfig, McpBridgeError> {
        self.servers
            .get(name)
            .ok_or_else(|| McpBridgeError::UnknownServer {
                server: name.to_owned(),
            })
    }

    async fn session_for(
        &self,
        server: &McpServerConfig,
        cancel: &CancellationToken,
    ) -> Result<Arc<ServerSession>, McpBridgeError> {
        if cancel.is_cancelled() || self.shutdown.is_cancelled() {
            return Err(McpBridgeError::Cancelled);
        }

        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(&server.name) {
                let guard = session.service.lock().await;
                if guard.is_some() {
                    return Ok(session.clone());
                }
            }
        }

        let session = Arc::new(ServerSession {
            service: Mutex::new(Some(connect_server(server, cancel.child_token()).await?)),
        });

        let mut sessions = self.sessions.write().await;
        sessions.insert(server.name.clone(), session.clone());
        Ok(session)
    }
}

#[async_trait]
impl McpToolClient for RmcpToolClient {
    async fn list_tools(
        &self,
        server: &McpServerConfig,
        cancel: CancellationToken,
    ) -> Result<Vec<McpRemoteTool>, McpBridgeError> {
        let span = tracing::info_span!("mcp.list_tools", server = %server.name);
        let _guard = span.enter();

        self.server_config(&server.name)?;
        let session = self.session_for(server, &cancel).await?;
        let guard = session.service.lock().await;
        let service = guard.as_ref().ok_or_else(|| McpBridgeError::Connection {
            server: server.name.clone(),
            message: "session is closed".to_owned(),
        })?;
        let tools = service
            .peer()
            .list_all_tools()
            .await
            .map_err(|err| map_service_error(&server.name, err))?;

        Ok(tools
            .into_iter()
            .map(|tool| map_remote_tool(&server.name, tool))
            .collect())
    }

    async fn call_tool(
        &self,
        call: McpToolCall,
        cancel: CancellationToken,
    ) -> Result<ToolOutput, McpBridgeError> {
        let span = tracing::info_span!(
            "mcp.call_tool",
            server = %call.tool_ref.server_name,
            tool = %call.tool_ref.tool_name,
        );
        let _guard = span.enter();

        let server = self.server_config(&call.tool_ref.server_name)?.clone();
        let arguments = match call.input {
            serde_json::Value::Object(map) => map,
            serde_json::Value::Null => serde_json::Map::new(),
            other => {
                return Err(McpBridgeError::ToolCall(format!(
                    "MCP tool `{}` expects a JSON object input, got {other}",
                    call.tool_ref.tool_name
                )));
            }
        };

        let session = self.session_for(&server, &cancel).await?;
        let guard = session.service.lock().await;
        let service = guard.as_ref().ok_or_else(|| McpBridgeError::Connection {
            server: server.name.clone(),
            message: "session is closed".to_owned(),
        })?;
        let result = service
            .peer()
            .call_tool(
                CallToolRequestParams::new(call.tool_ref.tool_name).with_arguments(arguments),
            )
            .await
            .map_err(|err| map_service_error(&server.name, err))?;

        Ok(map_call_result(result).into_tool_output())
    }

    async fn shutdown(&self) {
        self.close_connections().await;
    }
}

async fn connect_server(
    server: &McpServerConfig,
    cancel: CancellationToken,
) -> Result<ClientSession, McpBridgeError> {
    let span = tracing::info_span!("mcp.connect", server = %server.name);
    let _guard = span.enter();

    let result = match &server.transport {
        McpServerTransport::Stdio(config) => {
            let command = tokio::process::Command::new(&config.command).configure(|cmd| {
                cmd.args(&config.args);
                for (key, value) in &config.env {
                    cmd.env(key, value);
                }
                if let Some(cwd) = &config.cwd {
                    cmd.current_dir(cwd);
                }
                cmd.stderr(Stdio::null());
                #[cfg(unix)]
                cmd.process_group(0);
                #[cfg(windows)]
                cmd.creation_flags(0x0800_0000);
            });
            let transport =
                TokioChildProcess::new(command).map_err(|err| McpBridgeError::Transport {
                    server: server.name.clone(),
                    message: err.to_string(),
                })?;
            serve_client_with_ct((), transport, cancel).await
        }
        McpServerTransport::StreamableHttp(config) | McpServerTransport::Sse(config) => {
            let headers = http_headers(&server.name, &config.headers)?;
            let transport = StreamableHttpClientTransport::from_config(
                StreamableHttpClientTransportConfig::with_uri(config.url.clone())
                    .custom_headers(headers),
            );
            serve_client_with_ct((), transport, cancel).await
        }
    };

    match result {
        Ok(service) => Ok(service),
        Err(ClientInitializeError::Cancelled) => Err(McpBridgeError::Cancelled),
        Err(err) => Err(McpBridgeError::Connection {
            server: server.name.clone(),
            message: err.to_string(),
        }),
    }
}

fn http_headers(
    server: &str,
    headers: &BTreeMap<String, String>,
) -> Result<HashMap<http::HeaderName, http::HeaderValue>, McpBridgeError> {
    let mut parsed = HashMap::new();
    for (name, value) in headers {
        let header_name = http::HeaderName::from_bytes(name.as_bytes()).map_err(|err| {
            McpBridgeError::InvalidHeader {
                server: server.to_owned(),
                name: name.clone(),
                message: err.to_string(),
            }
        })?;
        let header_value =
            http::HeaderValue::from_str(value).map_err(|err| McpBridgeError::InvalidHeader {
                server: server.to_owned(),
                name: name.clone(),
                message: err.to_string(),
            })?;
        parsed.insert(header_name, header_value);
    }
    Ok(parsed)
}

fn map_remote_tool(server_name: &str, tool: RmcpTool) -> McpRemoteTool {
    let read_only = tool
        .annotations
        .as_ref()
        .and_then(|annotations| annotations.read_only_hint)
        .unwrap_or(false);
    let input_schema = tool.schema_as_json_value();
    McpRemoteTool {
        server_name: server_name.to_owned(),
        name: tool.name.into_owned(),
        description: tool.description.map(Cow::into_owned).unwrap_or_default(),
        input_schema,
        read_only,
    }
}

fn map_call_result(result: CallToolResult) -> McpToolResult {
    McpToolResult {
        content: result.content.into_iter().map(map_content).collect(),
        is_error: result.is_error.unwrap_or(false),
        structured: result.structured_content,
    }
}

fn map_content(block: ContentBlock) -> McpContent {
    match block {
        ContentBlock::Text(text) => McpContent::Text { text: text.text },
        other => McpContent::Json {
            value: serde_json::to_value(other).unwrap_or_else(|err| {
                serde_json::json!({ "error": format!("failed to encode MCP content: {err}") })
            }),
        },
    }
}

fn map_service_error(server: &str, err: ServiceError) -> McpBridgeError {
    match err {
        ServiceError::Cancelled { .. } => McpBridgeError::Cancelled,
        other => McpBridgeError::Transport {
            server: server.to_owned(),
            message: other.to_string(),
        },
    }
}

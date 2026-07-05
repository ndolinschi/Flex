use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AcpJsonRpcId {
    String(String),
    Number(u64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AcpRequest {
    pub jsonrpc: String,
    pub id: AcpJsonRpcId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl AcpRequest {
    pub fn new(
        id: AcpJsonRpcId,
        method: impl Into<String>,
        params: impl Into<serde_json::Value>,
    ) -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            id,
            method: method.into(),
            params: Some(params.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AcpNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AcpClientCapabilities {
    #[serde(default)]
    pub mcp_servers: bool,
    #[serde(default)]
    pub filesystem_paths: bool,
    #[serde(default)]
    pub permissions: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AcpMcpServer {
    pub name: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AcpSessionNewParams {
    pub client_capabilities: AcpClientCapabilities,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<AcpMcpServer>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_json_rpc_request() {
        let request = AcpRequest::new(
            AcpJsonRpcId::Number(1),
            "session/new",
            serde_json::json!({ "cwd": "/tmp/work" }),
        );

        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.method, "session/new");
        assert_eq!(
            request.params,
            Some(serde_json::json!({ "cwd": "/tmp/work" }))
        );
    }
}

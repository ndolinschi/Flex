//! Static OpenAPI document for the desktop Remote Access `/v1` surface.

pub fn openapi_json() -> serde_json::Value {
    serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Desktop Remote Access API",
            "version": "1",
            "description": "In-process remote control plane for the desktop composition root. Clients pair via Settings (host/port/token or Bonjour) and call these routes with Authorization: Bearer <token>."
        },
        "paths": {
            "/health": {
                "get": {
                    "summary": "Liveness (no auth)",
                    "responses": { "200": { "description": "OK" } }
                }
            },
            "/v1/info": {
                "get": {
                    "summary": "Device + capability advertisement",
                    "security": [{ "bearer": [] }],
                    "responses": { "200": { "description": "InfoResponse" } }
                }
            },
            "/v1/openapi.json": {
                "get": {
                    "summary": "This document",
                    "security": [{ "bearer": [] }],
                    "responses": { "200": { "description": "OpenAPI JSON" } }
                }
            },
            "/v1/sessions": {
                "get": {
                    "summary": "List sessions",
                    "security": [{ "bearer": [] }],
                    "responses": { "200": { "description": "SessionSummary[]" } }
                },
                "post": {
                    "summary": "Create session",
                    "security": [{ "bearer": [] }],
                    "responses": { "200": { "description": "CreateSessionResponse" } }
                }
            },
            "/v1/sessions/{id}": {
                "get": {
                    "summary": "Get session",
                    "security": [{ "bearer": [] }],
                    "responses": { "200": { "description": "SessionSummary" } }
                },
                "patch": {
                    "summary": "Update session",
                    "security": [{ "bearer": [] }],
                    "responses": { "200": { "description": "SessionSummary" } }
                },
                "delete": {
                    "summary": "Delete session",
                    "security": [{ "bearer": [] }],
                    "responses": { "204": { "description": "Deleted" } }
                }
            },
            "/v1/sessions/{id}/resume": {
                "post": {
                    "summary": "Resume session",
                    "security": [{ "bearer": [] }],
                    "responses": { "204": { "description": "Resumed" } }
                }
            },
            "/v1/sessions/{id}/prompt": {
                "post": {
                    "summary": "Admit a turn (202); watch /events",
                    "security": [{ "bearer": [] }],
                    "responses": { "202": { "description": "Accepted" } }
                }
            },
            "/v1/sessions/{id}/cancel": {
                "post": {
                    "summary": "Cancel in-flight turn",
                    "security": [{ "bearer": [] }],
                    "responses": { "204": { "description": "Cancelled" } }
                }
            },
            "/v1/sessions/{id}/events": {
                "get": {
                    "summary": "SSE replay-then-tail (includes streaming deltas)",
                    "security": [{ "bearer": [] }],
                    "parameters": [{
                        "name": "from_seq",
                        "in": "query",
                        "schema": { "type": "integer", "default": 0 }
                    }],
                    "responses": { "200": { "description": "text/event-stream" } }
                }
            },
            "/v1/sessions/{id}/permissions/{request_id}/resolve": {
                "post": {
                    "summary": "Resolve a permission ask",
                    "security": [{ "bearer": [] }],
                    "responses": { "204": { "description": "Resolved" } }
                }
            },
            "/v1/sessions/{id}/questions/{request_id}/respond": {
                "post": {
                    "summary": "Respond to AskUserQuestion",
                    "security": [{ "bearer": [] }],
                    "responses": { "204": { "description": "Answered" } }
                }
            },
            "/v1/mcp/servers": {
                "get": {
                    "summary": "List MCP servers",
                    "security": [{ "bearer": [] }],
                    "responses": { "200": { "description": "McpServerBody[]" } }
                },
                "put": {
                    "summary": "Upsert MCP server",
                    "security": [{ "bearer": [] }],
                    "responses": { "204": { "description": "Saved" } }
                }
            },
            "/v1/mcp/servers/{id}": {
                "delete": {
                    "summary": "Remove MCP server",
                    "security": [{ "bearer": [] }],
                    "responses": { "204": { "description": "Removed" } }
                }
            },
            "/v1/mcp/servers/{id}/test": {
                "post": {
                    "summary": "Test MCP server (list tools)",
                    "security": [{ "bearer": [] }],
                    "responses": { "200": { "description": "tool name[]" } }
                }
            },
            "/v1/providers": {
                "get": {
                    "summary": "Configured provider ids (no secrets)",
                    "security": [{ "bearer": [] }],
                    "responses": { "200": { "description": "string[]" } }
                }
            }
        },
        "components": {
            "securitySchemes": {
                "bearer": {
                    "type": "http",
                    "scheme": "bearer"
                }
            }
        }
    })
}

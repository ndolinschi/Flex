//! Static OpenAPI document for the desktop Remote Access `/v1` surface.
//!
//! Chat-only: list sessions, read filtered message events, send text prompts.

pub fn openapi_json() -> serde_json::Value {
    serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Desktop Remote Access API",
            "version": "1",
            "description": "Least-privilege chat companion for the desktop app. A remote client may only list root sessions, read scrubbed message events, and send text prompts with tools fully disabled. No MCP, session mutation, permissions, providers, or subagent access."
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
                    "summary": "List sessions (id + title only; no paths)",
                    "security": [{ "bearer": [] }],
                    "responses": { "200": { "description": "SessionSummary[]" } }
                }
            },
            "/v1/sessions/{id}": {
                "get": {
                    "summary": "Get session summary (id + title only)",
                    "security": [{ "bearer": [] }],
                    "responses": { "200": { "description": "SessionSummary" } }
                }
            },
            "/v1/sessions/{id}/prompt": {
                "post": {
                    "summary": "Send a text message (202). Tools auto-denied. Watch /events.",
                    "security": [{ "bearer": [] }],
                    "responses": { "202": { "description": "Accepted" } }
                }
            },
            "/v1/sessions/{id}/events": {
                "get": {
                    "summary": "SSE of chat messages only (tool/permission events filtered out)",
                    "security": [{ "bearer": [] }],
                    "parameters": [{
                        "name": "from_seq",
                        "in": "query",
                        "schema": { "type": "integer", "default": 0 }
                    }],
                    "responses": { "200": { "description": "text/event-stream" } }
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

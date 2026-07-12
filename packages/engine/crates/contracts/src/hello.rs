//! Protocol handshake. The first frame every transport sends.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::branding;
use crate::capability::AgentCaps;

/// Wire protocol version. Negotiated once at handshake, never per event.
/// Changes within a version are additive only; a breaking change bumps this
/// and keeps the previous version emittable for a deprecation window.
pub const PROTOCOL_VERSION: u32 = 1;

/// Identity of the serving engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EngineIdentity {
    pub name: String,
    pub version: String,
}

/// Handshake frame: protocol version, engine identity, and the capabilities
/// of the agent implementation serving this connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Hello {
    pub protocol_version: u32,
    pub engine: EngineIdentity,
    pub capabilities: AgentCaps,
}

impl Hello {
    pub fn new(capabilities: AgentCaps) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            engine: EngineIdentity {
                name: branding::PRODUCT_NAME.to_owned(),
                version: branding::ENGINE_VERSION.to_owned(),
            },
            capabilities,
        }
    }
}

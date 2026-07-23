use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::branding;
use crate::capability::AgentCaps;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EngineIdentity {
    pub name: String,
    pub version: String,
}

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

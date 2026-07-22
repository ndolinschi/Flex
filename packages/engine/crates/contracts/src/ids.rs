//! Newtype identifiers used across the event stream and session log.
//!
//! Generated ids are UUIDv7 (time-ordered), so logs sort chronologically by id.
//! All ids serialize as plain strings on the wire.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord,
            Serialize, Deserialize, JsonSchema,
        )]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            /// Generate a fresh time-ordered (UUIDv7) id.
            pub fn generate() -> Self {
                Self(uuid::Uuid::now_v7().to_string())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }
    };
}

id_type!(
    /// Identifies one session: an append-only event log owned by one agent.
    SessionId
);
id_type!(
    /// Identifies one turn: one user prompt and everything until the loop idles.
    TurnId
);
id_type!(
    /// Identifies one message (user or assistant) within a session.
    MessageId
);
id_type!(
    /// Identifies one tool invocation across its whole lifecycle.
    ToolCallId
);
id_type!(
    /// Identifies one permission request round-trip.
    PermissionRequestId
);
id_type!(
    /// Identifies one user-question round-trip.
    QuestionId
);
id_type!(
    /// Identifies one peer-to-peer agent message.
    PeerMessageId
);
id_type!(
    /// Identifies one composer mode-switch proposal round-trip.
    ModeSwitchId
);

/// Identifies an LLM provider implementation (`"anthropic"`, `"openai"`, ...).
///
/// Not a UUID — a stable, human-chosen key used in configuration, in
/// [`crate::content::ContentBlock::Opaque`] round-tripping, and in
/// provider-namespaced request passthrough.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct ProviderId(pub String);

impl ProviderId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ProviderId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for ProviderId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

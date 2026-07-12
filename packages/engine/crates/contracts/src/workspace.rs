//! Environment isolation: running a session inside an isolated working copy
//! (e.g. a git worktree) and integrating its changes back into the base tree.
//!
//! Pure data only — the mechanism (spawning `git`, copying files) lives at an
//! edge crate behind a `core` trait. These types are the vocabulary the
//! session log and clients speak.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Whether a root session runs inside an isolated working copy, and how
/// strictly that is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum IsolationPolicy {
    /// Never isolate; the session works directly in the base directory.
    #[default]
    Never,
    /// Isolate when possible; fall back to the base directory (with a warning)
    /// if a workspace can't be provisioned.
    Optional,
    /// Isolation is mandatory; session creation fails if a workspace can't be
    /// provisioned.
    Required,
}

impl IsolationPolicy {
    /// Whether this policy asks for isolation at all.
    pub fn wants_isolation(self) -> bool {
        matches!(self, Self::Optional | Self::Required)
    }

    /// Whether a provisioning failure must abort session creation.
    pub fn is_required(self) -> bool {
        matches!(self, Self::Required)
    }
}

/// What happened when an isolated workspace was integrated back into its base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
#[non_exhaustive]
pub enum IntegrationOutcome {
    /// Changes verified and merged back onto the base; the workspace was
    /// removed.
    Merged {
        /// Files changed, for a human-readable summary.
        files_changed: u32,
    },
    /// The verify step failed; the workspace was kept for review.
    VerifyFailed {
        /// Short reason / tail of the verify output.
        detail: String,
    },
    /// The base could not be fast-forwarded; the work was kept on a branch for
    /// a manual merge.
    Diverged {
        /// The branch holding the isolated work.
        branch: String,
    },
    /// Nothing to integrate — the workspace had no changes.
    Empty,
}

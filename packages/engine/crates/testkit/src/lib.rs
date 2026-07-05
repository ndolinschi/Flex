//! Test doubles and shared test infrastructure. Dev-dependency only — never
//! a runtime dependency of any shipping crate.
//!
//! Grows with the workspace: `MockProvider` and the provider conformance
//! suite arrive with the core traits; the scripted-stdio engine arrives with
//! the delegators.

pub mod fixtures;
pub mod mock_provider;
pub mod scenario;
pub mod tools;

pub use mock_provider::{MOCK_MODEL, MOCK_PROVIDER_ID, MockProvider, ScriptedError, ScriptedTurn};
pub use scenario::{ScenarioError, scenario_turns};
pub use tools::{EchoTool, FailingTool, PanickingTool, SlowTool};

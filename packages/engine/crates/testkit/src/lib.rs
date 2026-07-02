//! Test doubles and shared test infrastructure. Dev-dependency only — never
//! a runtime dependency of any shipping crate.
//!
//! Grows with the workspace: `MockProvider` and the provider conformance
//! suite arrive with the core traits; the scripted-stdio engine arrives with
//! the delegators.

pub mod fixtures;

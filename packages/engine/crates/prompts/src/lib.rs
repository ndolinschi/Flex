//! Prompt data as data: system-prompt assembly from composable markdown parts.
//!
//! The built-in prompt ships as numbered markdown files under
//! `packages/engine/prompts/system/`, embedded at compile time. Hosts can
//! override or extend individual parts via a directory of `*.md` files and
//! append free-form sections — see [`SystemPromptConfig`]. Assembly is
//! deterministic and side-effect free: placeholder values (`{{cwd}}`,
//! `{{date}}`) are passed in via [`Vars`], never read from the environment.
//!
//! ```
//! use agentloop_prompts::{SystemPromptAssembler, SystemPromptConfig, Vars};
//!
//! let assembler = SystemPromptAssembler::new(SystemPromptConfig::default());
//! let prompt = assembler.assemble(&Vars {
//!     cwd: "/workspace/project".to_owned(),
//!     date: "2026-01-01".to_owned(),
//! })?;
//! assert!(prompt.contains("/workspace/project"));
//! # Ok::<(), agentloop_prompts::PromptError>(())
//! ```

pub mod assembler;
pub mod commands;

pub use assembler::{PromptError, SystemPromptAssembler, SystemPromptConfig, Vars};
pub use commands::{CommandDiscoveryConfig, CommandError, CommandExpansion, CommandRegistry};

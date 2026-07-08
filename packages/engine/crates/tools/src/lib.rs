//! Base tool set. Arrives with M1.
//!
//! The crate exports concrete tools behind the [`agentloop_core::Tool`]
//! boundary plus one composition helper for the runner/engine front door.

mod agent;
mod ask_question;
mod bash;
mod exit_plan_mode;
pub mod fs;
mod glob;
mod grep;
mod plan;
mod registry;
mod skill;
mod web_fetch;

pub use agent::subagent_tool;
pub use ask_question::AskQuestionTool;
pub use bash::BashTool;
pub use exit_plan_mode::ExitPlanModeTool;
pub use fs::{EditTool, FsState, ReadTool, WriteTool, extract_page_links};
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use plan::PlanTool;
pub use registry::{BaseTools, base_tools, base_tools_read_only, registry_with_questions};
pub use skill::{SkillLoader, skill_tool};
pub use web_fetch::WebFetchTool;

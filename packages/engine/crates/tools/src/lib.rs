//! Base tool set. Arrives with M1.
//!
//! The crate exports concrete tools behind the [`agentloop_core::Tool`]
//! boundary plus one composition helper for the runner/engine front door.

mod ask_question;
mod bash;
mod exit_plan_mode;
pub mod fs;
mod glob;
mod grep;
mod registry;
mod skill;
mod task;
mod task_list;
mod web_fetch;

pub use ask_question::AskQuestionTool;
pub use bash::BashTool;
pub use exit_plan_mode::ExitPlanModeTool;
pub use fs::{EditTool, FsState, ReadTool, WriteTool};
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use registry::{BaseTools, base_tools, registry_with_questions};
pub use skill::{SkillLoader, skill_tool};
pub use task::subagent_tool;
pub use task_list::TaskListTool;
pub use web_fetch::WebFetchTool;

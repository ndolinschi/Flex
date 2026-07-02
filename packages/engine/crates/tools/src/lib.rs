//! Base tool set. Arrives with M1.
//!
//! The crate exports concrete tools behind the [`agentloop_core::Tool`]
//! boundary plus one composition helper for the runner/engine front door.

mod ask_question;
mod bash;
pub mod fs;
mod glob;
mod grep;
mod registry;
mod task_list;
mod web_fetch;

pub use ask_question::AskQuestionTool;
pub use bash::BashTool;
pub use fs::{EditTool, FsState, ReadTool, WriteTool};
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use registry::{BaseTools, base_tools, registry_with_questions};
pub use task_list::TaskListTool;
pub use web_fetch::WebFetchTool;

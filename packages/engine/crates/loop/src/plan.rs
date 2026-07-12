//! Plan-mode system-prompt guidance.
//!
//! Appended to the system prompt for any turn whose permission mode is
//! [`agentloop_contracts::PermissionMode::Plan`]. The permission policy blocks
//! mutation regardless, but telling the model up front — rather than letting it
//! discover the constraint by getting an edit denied — keeps it on the intended
//! research-then-plan path and points it at the `ExitPlanMode` handoff.

/// The guidance block appended to the system prompt while in plan mode.
pub(crate) fn guidance() -> &'static str {
    "Plan mode is ON — you are in a read-only research phase.\n\
     - Investigate freely with read-only tools: Read, Grep, Glob, and read-only \
       shell such as `git log`, `git diff`, `git status`, `ls`, `cat`, `rg`.\n\
     - Do NOT modify files, run mutating shell commands, or make any other \
       changes — such actions are blocked in plan mode and will be denied.\n\
     - When you have gathered enough context, write a clear, step-by-step \
       implementation plan, then call the `ExitPlanMode` tool with that plan to \
       hand it to the user for approval. Do not start implementing until the \
       user approves and leaves plan mode."
}

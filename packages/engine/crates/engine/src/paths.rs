//! Default user-config paths and loop-limit helpers.

use std::path::PathBuf;

use agentloop_loop::LoopLimits;

pub(crate) fn default_user_command_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("agentloop")
            .join("commands")
    })
}

pub(crate) fn default_user_memory_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("agentloop")
            .join("memory")
    })
}

pub(crate) fn default_user_skill_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("agentloop")
            .join("skills")
    })
}

/// A configured `max_iterations` overrides the loop's own default; `None`
/// keeps it.
pub(crate) fn resolve_max_iterations(configured: Option<u32>) -> u32 {
    configured.unwrap_or(LoopLimits::default().max_iterations)
}

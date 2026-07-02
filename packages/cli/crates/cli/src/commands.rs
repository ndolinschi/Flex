//! Slash-command routing and the merged autocomplete index.
//!
//! Two namespaces share one popup: CLI-local commands (intercepted before
//! `prompt()`) and engine-provided commands from the `hello()` handshake
//! (sent through as prompt text; the engine expands them). On a name
//! collision the CLI wins — deterministic and documented in `/help`.

use agentloop_contracts::CommandInfo;

/// A CLI-local command, parsed from the input line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalCommand {
    /// `/model [ref]` — pick or set the session model.
    Model { arg: Option<String> },
    /// `/provider [name]` — pick a provider; sets its default model.
    Provider { arg: Option<String> },
    /// `/agent [id]` — switch agent implementation.
    Agent { arg: Option<String> },
    /// `/new` — fresh session on the current service.
    New,
    /// `/help` — keys and command list.
    Help,
    /// `/command <shell>` — run a shell command in the session cwd.
    Command { shell: String },
    /// `/copy` — copy the chat transcript to the clipboard.
    Copy,
    /// `/quit`, `/exit`.
    Quit,
    /// `/mode [code|plan]` — session mode (research vs normal).
    Mode { arg: Option<String> },
    /// `/permissions [require|auto|allow-all]` — security level.
    Permissions { arg: Option<String> },
    /// `/thinking [off|low|medium|high|on|off]` — budget or visibility.
    Thinking { arg: Option<String> },
    /// `/compact` — summarize conversation history to save context.
    Compact,
    /// `/mcps` — list and toggle installed MCP servers.
    Mcps,
    /// `/mcp <name>` or `/mcp explore <name>`.
    Mcp { sub: McpSubcommand },
    /// `/mcp-install` — install MCP servers.
    McpInstall { arg: Option<String> },
    /// `/mcp-remove <name>` — remove an installed server.
    McpRemove { name: String },
}

/// `/mcp` subcommands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpSubcommand {
    /// Enable and attach `name` to the session.
    Attach { name: String },
    /// Open the tool explorer for `name`.
    Explore { name: String },
}

/// Where a submitted line goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// Intercepted by the CLI.
    Local(LocalCommand),
    /// A known engine command; send as prompt text for engine expansion.
    Engine,
    /// Plain prompt text (including unknown `/x`, sent literally).
    Plain,
}

/// One autocomplete entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandEntry {
    /// Name without the leading slash.
    pub name: String,
    pub description: String,
    pub args_hint: Option<String>,
    /// Dim source tag: `cli`, `builtin`, `user`, `project`, `agent`.
    pub source: &'static str,
}

/// CLI-local command table: `(name, description, args_hint)`.
const LOCAL: &[(&str, &str, Option<&str>)] = &[
    (
        "model",
        "pick or set the session model",
        Some("[provider/model]"),
    ),
    ("models", "alias of /model", None),
    (
        "provider",
        "pick a provider (sets its default model)",
        Some("[name]"),
    ),
    (
        "agent",
        "switch agent implementation",
        Some("[native|claude-code|copilot]"),
    ),
    ("new", "start a fresh session", None),
    (
        "command",
        "run a shell command in the current directory",
        Some("<shell command>"),
    ),
    ("help", "keys and commands", None),
    ("copy", "copy chat transcript to clipboard", None),
    ("quit", "exit", None),
    ("mode", "switch code/plan session mode", Some("[code|plan]")),
    (
        "permissions",
        "set permission security level",
        Some("[require|auto|allow-all]"),
    ),
    (
        "thinking",
        "show or hide reasoning blocks",
        Some("[on|off]"),
    ),
    (
        "compact",
        "summarize conversation history to save context",
        None,
    ),
    ("mcps", "list and toggle installed MCP servers", None),
    (
        "mcp",
        "attach or explore an MCP server",
        Some("<name> | explore <name>"),
    ),
    (
        "mcp-install",
        "install MCP servers (registry, npm, or import)",
        Some("[repo|npm]"),
    ),
    (
        "mcp-remove",
        "remove an installed MCP server",
        Some("<name>"),
    ),
];

/// Merged local + engine command list with routing and filtering.
#[derive(Debug, Default)]
pub struct CommandIndex {
    entries: Vec<CommandEntry>,
    engine_names: Vec<String>,
}

impl CommandIndex {
    /// Build from the engine's declared commands. Local commands come first;
    /// engine commands shadowed by a local name are dropped from the list.
    pub fn new(engine_commands: &[CommandInfo]) -> Self {
        let mut entries: Vec<CommandEntry> = LOCAL
            .iter()
            .map(|(name, description, hint)| CommandEntry {
                name: (*name).to_owned(),
                description: (*description).to_owned(),
                args_hint: hint.map(str::to_owned),
                source: "cli",
            })
            .collect();
        let mut engine_names = Vec::new();
        for info in engine_commands {
            engine_names.push(info.name.clone());
            if LOCAL.iter().any(|(name, ..)| *name == info.name) {
                continue;
            }
            entries.push(CommandEntry {
                name: info.name.clone(),
                description: info.description.clone(),
                args_hint: info.args_hint.clone(),
                source: source_label(info),
            });
        }
        Self {
            entries,
            engine_names,
        }
    }

    /// All entries, for `/help`.
    pub fn entries(&self) -> &[CommandEntry] {
        &self.entries
    }

    /// Fuzzy-prefix filter: prefix matches rank first, then subsequence
    /// matches, each group alphabetical.
    pub fn matches(&self, filter: &str) -> Vec<CommandEntry> {
        let filter = filter.to_lowercase();
        let mut ranked: Vec<(u8, &CommandEntry)> = self
            .entries
            .iter()
            .filter_map(|entry| {
                let name = entry.name.to_lowercase();
                if name.starts_with(&filter) {
                    Some((0, entry))
                } else if is_subsequence(&filter, &name) {
                    Some((1, entry))
                } else {
                    None
                }
            })
            .collect();
        ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
        ranked.into_iter().map(|(_, entry)| entry.clone()).collect()
    }

    /// Decide where a submitted line goes.
    pub fn route(&self, line: &str) -> Route {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix('/') else {
            return Route::Plain;
        };
        let (name, args) = match rest.split_once(char::is_whitespace) {
            Some((name, args)) => (name, args.trim()),
            None => (rest, ""),
        };
        let arg = (!args.is_empty()).then(|| args.to_owned());
        match name {
            "model" | "models" => Route::Local(LocalCommand::Model { arg }),
            "provider" => Route::Local(LocalCommand::Provider { arg }),
            "agent" => Route::Local(LocalCommand::Agent { arg }),
            "new" => Route::Local(LocalCommand::New),
            "help" => Route::Local(LocalCommand::Help),
            "copy" => Route::Local(LocalCommand::Copy),
            "command" => Route::Local(LocalCommand::Command {
                shell: args.to_owned(),
            }),
            "quit" | "exit" => Route::Local(LocalCommand::Quit),
            "mode" => Route::Local(LocalCommand::Mode { arg }),
            "permissions" => Route::Local(LocalCommand::Permissions { arg }),
            "thinking" => Route::Local(LocalCommand::Thinking { arg }),
            "compact" => Route::Local(LocalCommand::Compact),
            "mcps" => Route::Local(LocalCommand::Mcps),
            "mcp" => Route::Local(parse_mcp_command(args)),
            "mcp-install" => Route::Local(LocalCommand::McpInstall { arg: arg.clone() }),
            "mcp-remove" => Route::Local(parse_mcp_remove(args)),
            other if self.engine_names.iter().any(|n| n == other) => Route::Engine,
            _ => Route::Plain,
        }
    }
}

fn source_label(info: &CommandInfo) -> &'static str {
    use agentloop_contracts::CommandSource;
    match info.source {
        CommandSource::Builtin => "builtin",
        CommandSource::User => "user",
        CommandSource::Project => "project",
        CommandSource::Agent => "agent",
        _ => "engine",
    }
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    needle.chars().all(|n| chars.any(|h| h == n))
}

fn parse_mcp_command(args: &str) -> LocalCommand {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return LocalCommand::Mcps;
    }
    if let Some(name) = trimmed.strip_prefix("explore ") {
        let name = name.trim();
        if name.is_empty() {
            return LocalCommand::Mcps;
        }
        return LocalCommand::Mcp {
            sub: McpSubcommand::Explore {
                name: name.to_owned(),
            },
        };
    }
    LocalCommand::Mcp {
        sub: McpSubcommand::Attach {
            name: trimmed.to_owned(),
        },
    }
}

fn parse_mcp_remove(args: &str) -> LocalCommand {
    LocalCommand::McpRemove {
        name: args.trim().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_copy_is_local() {
        let index = CommandIndex::default();
        assert_eq!(index.route("/copy"), Route::Local(LocalCommand::Copy));
    }

    #[test]
    fn registry_lists_copy() {
        let index = CommandIndex::new(&[]);
        assert!(index.entries().iter().any(|e| e.name == "copy"));
    }

    #[test]
    fn route_command_parses_shell_remainder() {
        let index = CommandIndex::default();
        assert_eq!(
            index.route("/command ls -la"),
            Route::Local(LocalCommand::Command {
                shell: "ls -la".to_owned(),
            })
        );
        assert_eq!(
            index.route("/command echo hello world"),
            Route::Local(LocalCommand::Command {
                shell: "echo hello world".to_owned(),
            })
        );
    }

    #[test]
    fn route_command_without_args() {
        let index = CommandIndex::default();
        assert_eq!(
            index.route("/command"),
            Route::Local(LocalCommand::Command {
                shell: String::new(),
            })
        );
    }

    #[test]
    fn command_appears_in_entries() {
        let index = CommandIndex::new(&[]);
        assert!(
            index
                .entries()
                .iter()
                .any(|entry| entry.name == "command" && entry.source == "cli")
        );
    }

    #[test]
    fn route_mode_plan() {
        let index = CommandIndex::default();
        assert_eq!(
            index.route("/mode plan"),
            Route::Local(LocalCommand::Mode {
                arg: Some("plan".to_owned()),
            })
        );
    }

    #[test]
    fn route_permissions_auto() {
        let index = CommandIndex::default();
        assert_eq!(
            index.route("/permissions auto"),
            Route::Local(LocalCommand::Permissions {
                arg: Some("auto".to_owned()),
            })
        );
    }

    #[test]
    fn route_thinking_on() {
        let index = CommandIndex::default();
        assert_eq!(
            index.route("/thinking on"),
            Route::Local(LocalCommand::Thinking {
                arg: Some("on".to_owned()),
            })
        );
    }

    #[test]
    fn route_compact_is_local() {
        let index = CommandIndex::default();
        assert_eq!(index.route("/compact"), Route::Local(LocalCommand::Compact));
    }

    #[test]
    fn route_permissions_allow_all() {
        let index = CommandIndex::default();
        assert_eq!(
            index.route("/permissions allow-all"),
            Route::Local(LocalCommand::Permissions {
                arg: Some("allow-all".to_owned()),
            })
        );
    }

    #[test]
    fn route_mcp_explore() {
        let index = CommandIndex::default();
        assert_eq!(
            index.route("/mcp explore filesystem"),
            Route::Local(LocalCommand::Mcp {
                sub: McpSubcommand::Explore {
                    name: "filesystem".to_owned(),
                },
            })
        );
    }

    #[test]
    fn route_mcps_is_local() {
        let index = CommandIndex::default();
        assert_eq!(index.route("/mcps"), Route::Local(LocalCommand::Mcps));
    }
}

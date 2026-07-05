//! Slash-command discovery and expansion.
//!
//! Commands are prompt templates. The registry is deterministic, has no global
//! state, and treats missing user/project command directories as empty so a
//! host can opt into discovery without making startup fragile.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use agentloop_contracts::{CommandInfo, CommandSource, ContentBlock, ExpandedCommand, PromptInput};
use serde::Deserialize;

/// Where command files are discovered from.
#[derive(Debug, Clone, Default)]
pub struct CommandDiscoveryConfig {
    /// User-level command directory, typically under a config directory.
    pub user_dir: Option<PathBuf>,
    /// Project-level command directory, typically under the workspace root.
    pub project_dir: Option<PathBuf>,
}

/// A slash command after template expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandExpansion {
    pub name: String,
    pub args: String,
    pub text: String,
}

/// Errors from command discovery/expansion.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CommandError {
    #[error(
        "cannot read slash-command directory `{}`: {source}. \
         Remove the directory from discovery or make it readable.",
        path.display()
    )]
    Dir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "cannot read slash-command file `{}`: {source}. \
         Command files must be readable UTF-8 JSON.",
        path.display()
    )]
    File {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "slash-command file `{}` is not valid JSON: {source}. \
         Expected {{\"description\":\"...\",\"template\":\"...\",\"args_hint\":\"...\"}}.",
        path.display()
    )]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "slash-command file `{}` has no usable command name. \
         Use an ASCII file stem like `review.json` or set a non-empty `name`.",
        path.display()
    )]
    Name { path: PathBuf },
}

/// A deterministic registry of slash-command templates.
#[derive(Debug, Clone, Default)]
pub struct CommandRegistry {
    commands: BTreeMap<String, Command>,
}

#[derive(Debug, Clone)]
struct Command {
    info: CommandInfo,
    template: String,
}

#[derive(Debug, Deserialize)]
struct CommandFile {
    #[serde(default)]
    name: Option<String>,
    description: String,
    #[serde(default)]
    args_hint: Option<String>,
    template: String,
}

impl CommandRegistry {
    /// Built-ins only.
    pub fn builtins() -> Self {
        let mut registry = Self::default();
        registry.register(
            CommandSource::Builtin,
            "plan",
            "Create an implementation plan for the request.",
            Some("<request>"),
            "Create a concise implementation plan for the following request:\n\n{{args}}",
        );
        registry.register(
            CommandSource::Builtin,
            "review",
            "Review code or changes for bugs, regressions, and missing tests.",
            Some("<scope>"),
            "Review the following code or changes. Prioritize bugs, behavioral regressions, and missing tests:\n\n{{args}}",
        );
        registry.register(
            CommandSource::Builtin,
            "explain",
            "Explain the referenced code or concept.",
            Some("<topic>"),
            "Explain this clearly, including the important trade-offs and code references when relevant:\n\n{{args}}",
        );
        registry.register(
            CommandSource::Builtin,
            "fix",
            "Investigate and fix the described issue.",
            Some("<issue>"),
            "Investigate and fix the following issue. Keep the change scoped and verify it:\n\n{{args}}",
        );
        registry
    }

    /// Built-ins plus optional user/project command directories. Later sources
    /// override earlier ones by command name, so project commands can customize
    /// a built-in without changing clients.
    pub fn discover(config: CommandDiscoveryConfig) -> Result<Self, CommandError> {
        let mut registry = Self::builtins();
        if let Some(dir) = config.user_dir {
            registry.discover_dir(&dir, CommandSource::User)?;
        }
        if let Some(dir) = config.project_dir {
            registry.discover_dir(&dir, CommandSource::Project)?;
        }
        Ok(registry)
    }

    /// Autocomplete-capability entries in deterministic name order.
    pub fn infos(&self) -> Vec<CommandInfo> {
        self.commands
            .values()
            .map(|command| command.info.clone())
            .collect()
    }

    /// Expand the first markdown block when it begins with a known slash
    /// command. Unknown commands pass through unchanged.
    pub fn expand_input(&self, mut input: PromptInput) -> PromptInput {
        let Some((index, text)) = first_markdown(&input.parts) else {
            return input;
        };
        let Some(parsed) = parse_invocation(text) else {
            return input;
        };
        let Some(expansion) = self.expand(parsed.name, parsed.args, parsed.rest) else {
            return input;
        };

        input.parts[index] = ContentBlock::markdown(expansion.text);
        input.command = Some(ExpandedCommand {
            name: expansion.name,
            args: expansion.args,
        });
        input
    }

    /// Expand raw command pieces; exposed for focused tests and transports
    /// that parse command lines before constructing [`PromptInput`].
    pub fn expand(&self, name: &str, args: &str, rest: &str) -> Option<CommandExpansion> {
        let command = self.commands.get(name)?;
        let args = args.trim().to_owned();
        let mut text = command
            .template
            .replace("{{args}}", &args)
            .replace("{{trimmed_args}}", &args);
        let rest = rest.trim();
        if !rest.is_empty() {
            if !text.trim().is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(rest);
        }
        Some(CommandExpansion {
            name: name.to_owned(),
            args,
            text,
        })
    }

    fn register(
        &mut self,
        source: CommandSource,
        name: &str,
        description: &str,
        args_hint: Option<&str>,
        template: &str,
    ) {
        self.commands.insert(
            name.to_owned(),
            Command {
                info: CommandInfo {
                    name: name.to_owned(),
                    description: description.to_owned(),
                    args_hint: args_hint.map(str::to_owned),
                    source,
                },
                template: template.to_owned(),
            },
        );
    }

    fn discover_dir(&mut self, dir: &Path, source: CommandSource) -> Result<(), CommandError> {
        if !dir.exists() {
            return Ok(());
        }
        let entries = fs::read_dir(dir).map_err(|source| CommandError::Dir {
            path: dir.to_path_buf(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| CommandError::Dir {
                path: dir.to_path_buf(),
                source,
            })?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") || !path.is_file() {
                continue;
            }
            let raw = fs::read_to_string(&path).map_err(|source| CommandError::File {
                path: path.clone(),
                source,
            })?;
            let file: CommandFile =
                serde_json::from_str(&raw).map_err(|source| CommandError::Json {
                    path: path.clone(),
                    source,
                })?;
            let name = file
                .name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .or_else(|| {
                    path.file_stem()
                        .and_then(|stem| stem.to_str())
                        .map(str::trim)
                        .filter(|stem| !stem.is_empty())
                        .map(str::to_owned)
                })
                .ok_or_else(|| CommandError::Name { path: path.clone() })?;
            self.register(
                source,
                &name,
                &file.description,
                file.args_hint.as_deref(),
                &file.template,
            );
        }
        Ok(())
    }
}

struct ParsedInvocation<'a> {
    name: &'a str,
    args: &'a str,
    rest: &'a str,
}

fn first_markdown(parts: &[ContentBlock]) -> Option<(usize, &str)> {
    parts
        .iter()
        .enumerate()
        .find_map(|(index, part)| match part {
            ContentBlock::Markdown { text } => Some((index, text.as_str())),
            _ => None,
        })
}

fn parse_invocation(text: &str) -> Option<ParsedInvocation<'_>> {
    let text = text.strip_prefix('/')?;
    let (line, rest) = match text.split_once('\n') {
        Some((line, rest)) => (line, rest),
        None => (text, ""),
    };
    let line = line.trim_end();
    let split_at = line.find(char::is_whitespace).unwrap_or(line.len());
    let (name, args) = line.split_at(split_at);
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return None;
    }
    Some(ParsedInvocation {
        name,
        args: args.trim_start(),
        rest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[test]
    fn builtins_expand_first_markdown_block() {
        let input = PromptInput::text("/explain crates/loop");

        let expanded = CommandRegistry::builtins().expand_input(input);

        assert_eq!(
            expanded.command,
            Some(ExpandedCommand {
                name: "explain".to_owned(),
                args: "crates/loop".to_owned()
            })
        );
        assert!(expanded.joined_text().contains("crates/loop"));
        assert!(expanded.joined_text().contains("Explain this clearly"));
    }

    #[test]
    fn unknown_commands_pass_through() {
        let input = PromptInput::text("/missing hello");

        let expanded = CommandRegistry::builtins().expand_input(input.clone());

        assert_eq!(expanded, input);
    }

    #[test]
    fn project_commands_override_builtins() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("review.json"),
            r#"{
                "description": "Custom review",
                "args_hint": "<thing>",
                "template": "Custom template: {{args}}"
            }"#,
        )
        .expect("write command");

        let registry = CommandRegistry::discover(CommandDiscoveryConfig {
            user_dir: None,
            project_dir: Some(dir.path().to_path_buf()),
        })
        .expect("discover commands");
        let expanded = registry
            .expand("review", "src/lib.rs", "")
            .expect("review command exists");

        assert_eq!(expanded.text, "Custom template: src/lib.rs");
        let info = registry
            .infos()
            .into_iter()
            .find(|info| info.name == "review")
            .expect("review info exists");
        assert_eq!(info.source, CommandSource::Project);
    }

    #[test]
    fn missing_discovery_dirs_are_empty() {
        let registry = CommandRegistry::discover(CommandDiscoveryConfig {
            user_dir: Some(PathBuf::from("/definitely/missing/commands")),
            project_dir: None,
        })
        .expect("missing dirs are ignored");

        assert!(registry.expand("plan", "work", "").is_some());
    }
}

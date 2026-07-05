//! Hand-rolled command-line parsing for the interactive CLI.

use std::path::PathBuf;

use agentloop_cli_core::AgentKind;

/// What the process should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Launch the full-screen TUI.
    Tui { resume: ResumeMode },
    /// Run the Copilot device-flow login without entering the TUI.
    AuthLogin,
    /// Print recent sessions for the current directory and exit.
    ListSessions,
}

/// Which session the TUI should start from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeMode {
    /// Start a fresh session (the default).
    New,
    /// Resume the most recently updated session for this working directory.
    Continue,
    /// Resume a specific session by id.
    Session(String),
}

/// Parsed process configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub command: Command,
    pub agent: AgentKind,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub workdir: PathBuf,
    /// Effort level (`low|medium|high|xhigh|max`); validated at startup, where
    /// an unrecognized value surfaces an error and the saved default is kept.
    pub effort: Option<String>,
}

impl Args {
    /// Parse from `std::env::args()`.
    pub fn parse_env() -> Result<Self, ArgError> {
        Self::parse(std::env::args().skip(1))
    }

    /// Parse a sequence of argv tokens (excluding argv\[0\]).
    pub fn parse<I, S>(args: I) -> Result<Self, ArgError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut out = Self {
            command: Command::Tui {
                resume: ResumeMode::New,
            },
            agent: AgentKind::Native,
            provider: None,
            model: None,
            workdir: std::env::current_dir().map_err(|err| ArgError::Invalid {
                flag: "--workdir".to_owned(),
                message: format!("could not determine current directory: {err}"),
            })?,
            effort: None,
        };
        let mut iter = args.into_iter().map(Into::into).peekable();
        while let Some(arg) = iter.next() {
            if arg == "auth" {
                match iter.next().as_deref() {
                    Some("login") => out.command = Command::AuthLogin,
                    Some(other) => {
                        return Err(ArgError::Invalid {
                            flag: "auth".to_owned(),
                            message: format!("unknown auth subcommand `{other}`"),
                        });
                    }
                    None => {
                        return Err(ArgError::Invalid {
                            flag: "auth".to_owned(),
                            message: "expected `login`".to_owned(),
                        });
                    }
                }
                continue;
            }
            if arg == "sessions" {
                out.command = Command::ListSessions;
                continue;
            }

            let (flag, inline) = split_flag(&arg);
            match flag {
                "--continue" | "-c" => {
                    out.command = Command::Tui {
                        resume: ResumeMode::Continue,
                    };
                }
                "--resume" => {
                    let value = inline
                        .or_else(|| iter.next())
                        .ok_or_else(|| ArgError::MissingValue("--resume".to_owned()))?;
                    out.command = Command::Tui {
                        resume: ResumeMode::Session(value),
                    };
                }
                "--agent" => {
                    let value = inline
                        .or_else(|| iter.next())
                        .ok_or_else(|| ArgError::MissingValue("--agent".to_owned()))?;
                    out.agent = AgentKind::parse(&value).ok_or_else(|| ArgError::Invalid {
                        flag: "--agent".to_owned(),
                        message: format!(
                            "unknown agent `{value}` (expected native, claude-code, or copilot)"
                        ),
                    })?;
                }
                "--provider" => {
                    out.provider = Some(
                        inline
                            .or_else(|| iter.next())
                            .ok_or_else(|| ArgError::MissingValue("--provider".to_owned()))?,
                    );
                }
                "--model" => {
                    out.model = Some(
                        inline
                            .or_else(|| iter.next())
                            .ok_or_else(|| ArgError::MissingValue("--model".to_owned()))?,
                    );
                }
                "--effort" => {
                    out.effort = Some(
                        inline
                            .or_else(|| iter.next())
                            .ok_or_else(|| ArgError::MissingValue("--effort".to_owned()))?,
                    );
                }
                "--workdir" | "--cwd" => {
                    let value = inline
                        .or_else(|| iter.next())
                        .ok_or_else(|| ArgError::MissingValue(flag.to_owned()))?;
                    out.workdir = PathBuf::from(value);
                }
                "-h" | "--help" => return Err(ArgError::Help),
                other => return Err(ArgError::Unknown(other.to_owned())),
            }
        }
        Ok(out)
    }
}

fn split_flag(arg: &str) -> (&str, Option<String>) {
    match arg.split_once('=') {
        Some((flag, value)) => (flag, Some(value.to_owned())),
        None => (arg, None),
    }
}

/// Parse failure.
#[derive(Debug, thiserror::Error)]
pub enum ArgError {
    #[error("missing value for {0}")]
    MissingValue(String),
    #[error("unknown argument `{0}`")]
    Unknown(String),
    #[error("{flag}: {message}")]
    Invalid { flag: String, message: String },
    #[error("usage requested")]
    Help,
}

/// Short usage text.
pub fn usage() -> &'static str {
    "usage: flex [--agent native|claude-code|copilot] [--provider id] [--model ref] [--effort low|medium|high|xhigh|max] [--workdir path] [--continue | -c] [--resume <id>]\n       flex sessions\n       flex auth login"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_command_starts_a_new_session() {
        let args = Args::parse(Vec::<&str>::new()).expect("parses");
        assert_eq!(
            args.command,
            Command::Tui {
                resume: ResumeMode::New
            }
        );
    }

    #[test]
    fn continue_flag_and_short_alias_resume_the_most_recent_session() {
        for flag in ["--continue", "-c"] {
            let args = Args::parse([flag]).expect("parses");
            assert_eq!(
                args.command,
                Command::Tui {
                    resume: ResumeMode::Continue
                }
            );
        }
    }

    #[test]
    fn resume_flag_takes_an_explicit_session_id() {
        let args = Args::parse(["--resume", "abc-123"]).expect("parses");
        assert_eq!(
            args.command,
            Command::Tui {
                resume: ResumeMode::Session("abc-123".to_owned())
            }
        );
    }

    #[test]
    fn resume_flag_accepts_inline_value() {
        let args = Args::parse(["--resume=abc-123"]).expect("parses");
        assert_eq!(
            args.command,
            Command::Tui {
                resume: ResumeMode::Session("abc-123".to_owned())
            }
        );
    }

    #[test]
    fn resume_flag_without_a_value_is_an_error() {
        let err = Args::parse(["--resume"]).expect_err("missing value");
        assert!(matches!(err, ArgError::MissingValue(flag) if flag == "--resume"));
    }

    #[test]
    fn sessions_subcommand_sets_list_sessions() {
        let args = Args::parse(["sessions"]).expect("parses");
        assert_eq!(args.command, Command::ListSessions);
    }

    #[test]
    fn other_flags_still_parse_alongside_continue() {
        let args = Args::parse(["--continue", "--model", "anthropic/claude"]).expect("parses");
        assert_eq!(
            args.command,
            Command::Tui {
                resume: ResumeMode::Continue
            }
        );
        assert_eq!(args.model.as_deref(), Some("anthropic/claude"));
    }

    #[test]
    fn effort_flag_takes_a_value() {
        for spec in [
            vec!["--effort", "max"],
            vec!["--effort=max"],
        ] {
            let args = Args::parse(spec).expect("parses");
            assert_eq!(args.effort.as_deref(), Some("max"));
        }
    }

    #[test]
    fn effort_flag_without_a_value_is_an_error() {
        let err = Args::parse(["--effort"]).expect_err("missing value");
        assert!(matches!(err, ArgError::MissingValue(flag) if flag == "--effort"));
    }
}

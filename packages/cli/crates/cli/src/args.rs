//! Hand-rolled command-line parsing for the interactive CLI.

use std::path::PathBuf;

use agentloop_cli_core::AgentKind;

/// What the process should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Launch the full-screen TUI.
    Tui,
    /// Run the Copilot device-flow login without entering the TUI.
    AuthLogin,
}

/// Parsed process configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub command: Command,
    pub agent: AgentKind,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub workdir: PathBuf,
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
            command: Command::Tui,
            agent: AgentKind::Native,
            provider: None,
            model: None,
            workdir: std::env::current_dir().map_err(|err| ArgError::Invalid {
                flag: "--workdir".to_owned(),
                message: format!("could not determine current directory: {err}"),
            })?,
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

            let (flag, inline) = split_flag(&arg);
            match flag {
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
    "usage: agenticstudio [--agent native|claude-code|copilot] [--provider id] [--model ref] [--workdir path]\n       agenticstudio auth login"
}

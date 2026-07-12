//! Assembles the system prompt from ordered markdown parts.
//!
//! The built-in parts live in `packages/engine/prompts/system/` and are
//! embedded at compile time, so a deployed binary needs no data files. A
//! [`SystemPromptConfig::parts_dir`] can override individual built-ins (same
//! filename replaces) or contribute new parts (new filenames merge into the
//! filename sort order). Placeholder values are always passed in via
//! [`Vars`]; this crate never reads the clock or the environment.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// Built-in system prompt parts, embedded at compile time.
///
/// Ordered by filename; the numeric prefixes make the order explicit and
/// leave gaps for override directories to slot new parts in between.
const BUILT_IN_PARTS: [(&str, &str); 5] = [
    (
        "00-identity.md",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../prompts/system/00-identity.md"
        )),
    ),
    (
        "10-conduct.md",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../prompts/system/10-conduct.md"
        )),
    ),
    (
        "15-reasoning.md",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../prompts/system/15-reasoning.md"
        )),
    ),
    (
        "20-tool-use.md",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../prompts/system/20-tool-use.md"
        )),
    ),
    (
        "30-verification.md",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../prompts/system/30-verification.md"
        )),
    ),
];

/// Values substituted into `{{cwd}}` and `{{date}}` placeholders.
///
/// Callers supply both; the assembler never reads the clock or the process
/// environment, which keeps assembly deterministic and testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vars {
    /// Session working directory, substituted for `{{cwd}}`.
    pub cwd: String,
    /// Current date (caller-chosen format), substituted for `{{date}}`.
    pub date: String,
}

/// Configuration for [`SystemPromptAssembler`].
#[derive(Debug, Clone, Default)]
pub struct SystemPromptConfig {
    /// Optional directory of `*.md` parts. A file whose name matches a
    /// built-in part replaces it; other filenames are merged into the
    /// filename sort order.
    pub parts_dir: Option<PathBuf>,
    /// Free-form sections appended after all parts, in order.
    pub appends: Vec<String>,
}

/// Errors from system prompt assembly.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PromptError {
    /// The configured parts directory could not be read.
    #[error(
        "cannot read system prompt parts directory `{}`: {source}. \
         Create the directory or unset `parts_dir` to use the built-in prompt.",
        path.display()
    )]
    PartsDir {
        /// The directory that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A part file inside the parts directory could not be read.
    #[error(
        "cannot read system prompt part `{}`: {source}. \
         Ensure the file is readable UTF-8 or remove it from the parts directory.",
        path.display()
    )]
    PartFile {
        /// The part file that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Deterministically assembles the system prompt.
///
/// Assembly order: built-in parts sorted by filename, overridden/extended by
/// `parts_dir`, then each entry of `appends`. Non-empty sections are joined
/// with a blank line, and `{{cwd}}` / `{{date}}` placeholders are replaced
/// from [`Vars`].
#[derive(Debug, Clone, Default)]
pub struct SystemPromptAssembler {
    config: SystemPromptConfig,
}

impl SystemPromptAssembler {
    /// Creates an assembler with the given configuration.
    pub fn new(config: SystemPromptConfig) -> Self {
        Self { config }
    }

    /// Assembles the full system prompt.
    ///
    /// Given identical configuration, directory contents, and [`Vars`], the
    /// output is byte-for-byte identical across calls.
    pub fn assemble(&self, vars: &Vars) -> Result<String, PromptError> {
        let mut parts: BTreeMap<String, String> = BUILT_IN_PARTS
            .iter()
            .map(|(name, content)| ((*name).to_owned(), (*content).to_owned()))
            .collect();

        if let Some(dir) = &self.config.parts_dir {
            let entries = fs::read_dir(dir).map_err(|source| PromptError::PartsDir {
                path: dir.clone(),
                source,
            })?;
            for entry in entries {
                let entry = entry.map_err(|source| PromptError::PartsDir {
                    path: dir.clone(),
                    source,
                })?;
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("md") || !path.is_file() {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let content =
                    fs::read_to_string(&path).map_err(|source| PromptError::PartFile {
                        path: path.clone(),
                        source,
                    })?;
                parts.insert(name.to_owned(), content);
            }
        }

        let sections: Vec<&str> = parts
            .values()
            .map(String::as_str)
            .chain(self.config.appends.iter().map(String::as_str))
            .map(str::trim)
            .filter(|section| !section.is_empty())
            .collect();

        let assembled = sections.join("\n\n");
        Ok(assembled
            .replace("{{cwd}}", &vars.cwd)
            .replace("{{date}}", &vars.date))
    }
}

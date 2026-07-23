use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vars {
    pub cwd: String,
    pub date: String,
}

#[derive(Debug, Clone, Default)]
pub struct SystemPromptConfig {
    pub parts_dir: Option<PathBuf>,
    pub appends: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PromptError {
    #[error(
        "cannot read system prompt parts directory `{}`: {source}. \
         Create the directory or unset `parts_dir` to use the built-in prompt.",
        path.display()
    )]
    PartsDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "cannot read system prompt part `{}`: {source}. \
         Ensure the file is readable UTF-8 or remove it from the parts directory.",
        path.display()
    )]
    PartFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Default)]
pub struct SystemPromptAssembler {
    config: SystemPromptConfig,
}

impl SystemPromptAssembler {
    pub fn new(config: SystemPromptConfig) -> Self {
        Self { config }
    }

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

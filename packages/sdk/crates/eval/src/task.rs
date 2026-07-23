use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::EvalError;

fn default_timeout_secs() -> u64 {
    300
}

fn default_max_turns() -> u32 {
    1
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckSpec {
    #[serde(default)]
    pub cmd: Option<String>,

    #[serde(default)]
    pub expect_files: Vec<PathBuf>,

    #[serde(default)]
    pub expect_contains: BTreeMap<PathBuf, String>,
}

impl CheckSpec {
    pub fn is_empty(&self) -> bool {
        self.cmd.is_none() && self.expect_files.is_empty() && self.expect_contains.is_empty()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSpec {
    pub id: String,

    pub prompt: String,

    #[serde(default)]
    pub fixture: Option<PathBuf>,

    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,

    #[serde(default = "default_max_turns")]
    pub max_turns: u32,

    pub check: CheckSpec,
}

pub fn load_task(path: &Path) -> Result<TaskSpec, EvalError> {
    let text = std::fs::read_to_string(path)?;
    let mut task: TaskSpec = toml::from_str(&text).map_err(|err| EvalError::Task {
        path: path.to_path_buf(),
        message: err.to_string(),
    })?;
    let fail = |message: &str| EvalError::Task {
        path: path.to_path_buf(),
        message: message.to_owned(),
    };
    if task.id.trim().is_empty() {
        return Err(fail("`id` must not be empty"));
    }
    if task.prompt.trim().is_empty() {
        return Err(fail("`prompt` must not be empty"));
    }
    if task.max_turns == 0 {
        return Err(fail("`max_turns` must be at least 1"));
    }
    if task.timeout_secs == 0 {
        return Err(fail("`timeout_secs` must be at least 1"));
    }
    if task.check.is_empty() {
        return Err(fail(
            "[check] must set `cmd`, `expect_files`, or `expect_contains`",
        ));
    }
    if let Some(fixture) = task.fixture.take() {
        task.fixture = Some(resolve_fixture(path, &fixture)?);
    }
    Ok(task)
}

fn resolve_fixture(task_path: &Path, fixture: &Path) -> Result<PathBuf, EvalError> {
    if fixture.is_absolute() {
        if fixture.is_dir() {
            return Ok(fixture.to_path_buf());
        }
        return Err(EvalError::Task {
            path: task_path.to_path_buf(),
            message: format!("fixture dir not found: {}", fixture.display()),
        });
    }
    let task_dir = task_path.parent().unwrap_or(Path::new("."));
    let mut candidates = Vec::new();
    if let Some(suite_root) = task_dir.parent() {
        candidates.push(suite_root.join(fixture));
    }
    candidates.push(task_dir.join(fixture));
    for candidate in &candidates {
        if candidate.is_dir() {
            return Ok(candidate.clone());
        }
    }
    Err(EvalError::Task {
        path: task_path.to_path_buf(),
        message: format!(
            "fixture dir not found: {} (tried {})",
            fixture.display(),
            candidates
                .iter()
                .map(|c| c.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })
}

pub fn discover_tasks(dir: &Path, filter: &[String]) -> Result<Vec<TaskSpec>, EvalError> {
    let mut tasks = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            tasks.push(load_task(&path)?);
        }
    }
    if !filter.is_empty() {
        tasks.retain(|t| filter.iter().any(|f| f == &t.id));
    }
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    if tasks.is_empty() {
        return Err(EvalError::NoTasks(dir.to_path_buf()));
    }
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_task(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).expect("write task");
        path
    }

    const MINIMAL: &str = r#"
id = "create-file"
prompt = "Create hello.txt"
[check]
cmd = "grep -qx 'hello world' hello.txt"
"#;

    #[test]
    fn loads_minimal_task_with_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_task(dir.path(), "create-file.toml", MINIMAL);
        let task = load_task(&path).expect("loads");
        assert_eq!(task.id, "create-file");
        assert_eq!(task.timeout_secs, 300);
        assert_eq!(task.max_turns, 1);
        assert!(task.fixture.is_none());
    }

    #[test]
    fn rejects_task_without_any_check() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_task(
            dir.path(),
            "bad.toml",
            "id = \"x\"\nprompt = \"y\"\n[check]\n",
        );
        assert!(matches!(load_task(&path), Err(EvalError::Task { .. })));
    }

    #[test]
    fn resolves_fixture_against_suite_root() {
        let root = tempfile::tempdir().expect("tempdir");
        let tasks_dir = root.path().join("tasks");
        let fixture_dir = root.path().join("fixtures").join("demo");
        std::fs::create_dir_all(&tasks_dir).expect("mkdir");
        std::fs::create_dir_all(&fixture_dir).expect("mkdir");
        let path = write_task(
            &tasks_dir,
            "demo.toml",
            r#"
id = "demo"
prompt = "p"
fixture = "fixtures/demo"
[check]
expect_files = ["out.txt"]
"#,
        );
        let task = load_task(&path).expect("loads");
        assert_eq!(task.fixture, Some(fixture_dir));
    }

    #[test]
    fn missing_fixture_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_task(
            dir.path(),
            "demo.toml",
            r#"
id = "demo"
prompt = "p"
fixture = "fixtures/nope"
[check]
cmd = "true"
"#,
        );
        assert!(matches!(load_task(&path), Err(EvalError::Task { .. })));
    }

    #[test]
    fn discovery_sorts_and_filters() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_task(
            dir.path(),
            "b.toml",
            "id = \"b\"\nprompt = \"p\"\n[check]\ncmd = \"true\"\n",
        );
        write_task(
            dir.path(),
            "a.toml",
            "id = \"a\"\nprompt = \"p\"\n[check]\ncmd = \"true\"\n",
        );
        let all = discover_tasks(dir.path(), &[]).expect("discovers");
        assert_eq!(
            all.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        let only_b = discover_tasks(dir.path(), &["b".to_owned()]).expect("filters");
        assert_eq!(only_b.len(), 1);
        assert_eq!(only_b[0].id, "b");
        assert!(matches!(
            discover_tasks(dir.path(), &["zzz".to_owned()]),
            Err(EvalError::NoTasks(_))
        ));
    }
}

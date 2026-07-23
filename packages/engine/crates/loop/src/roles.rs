use agentloop_contracts::{IsolationPolicy, ModelRef};
use agentloop_core::{ToolFilter, ToolRegistry};

pub const MAIN_ROLE: &str = "main";

pub const VERIFIER_ROLE: &str = "verifier";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleToolProfile {
    ReadOnly,
    Full,
    Allow(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoleSpec {
    pub name: String,
    pub models: Vec<ModelRef>,
    pub tools: RoleToolProfile,
    pub prompt: Option<String>,
    pub split: bool,
    pub max_parallel: usize,
    pub max_depth: u8,
    pub isolation: IsolationPolicy,
}

impl RoleSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            models: Vec::new(),
            tools: RoleToolProfile::ReadOnly,
            prompt: None,
            split: true,
            max_parallel: 4,
            max_depth: 1,
            isolation: IsolationPolicy::Never,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RoleError {
    #[error("role `{0}` has an invalid name (use a-z, 0-9, -, _; max 32 chars)")]
    InvalidName(String),
    #[error("role `{0}` is declared more than once")]
    Duplicate(String),
}

#[derive(Debug, Default)]
pub struct RoleRegistry {
    roles: std::collections::BTreeMap<String, RoleSpec>,
}

impl RoleRegistry {
    pub fn with_defaults(user: Vec<RoleSpec>) -> Result<Self, RoleError> {
        let mut registry = Self::default();
        for spec in builtin_roles() {
            registry.roles.insert(spec.name.clone(), spec);
        }
        let mut seen = std::collections::BTreeSet::new();
        for mut spec in user {
            if !valid_name(&spec.name) {
                return Err(RoleError::InvalidName(spec.name));
            }
            if !seen.insert(spec.name.clone()) {
                return Err(RoleError::Duplicate(spec.name));
            }
            if let Some(existing) = registry.roles.get(&spec.name) {
                let defaults = RoleSpec::new(&spec.name);
                if spec.models.is_empty() {
                    spec.models = existing.models.clone();
                }
                if spec.prompt.is_none() {
                    spec.prompt = existing.prompt.clone();
                }
                if matches!(spec.tools, RoleToolProfile::ReadOnly)
                    && !matches!(existing.tools, RoleToolProfile::ReadOnly)
                {
                    spec.tools = existing.tools.clone();
                }
                if spec.max_parallel == defaults.max_parallel {
                    spec.max_parallel = existing.max_parallel;
                }
                if spec.max_depth == defaults.max_depth {
                    spec.max_depth = existing.max_depth;
                }
                if spec.split == defaults.split {
                    spec.split = existing.split;
                }
                if spec.isolation == defaults.isolation {
                    spec.isolation = existing.isolation;
                }
            }
            spec.max_parallel = spec.max_parallel.clamp(1, 8);
            spec.max_depth = spec.max_depth.min(3);
            registry.roles.insert(spec.name.clone(), spec);
        }
        Ok(registry)
    }

    pub fn get(&self, name: &str) -> Option<&RoleSpec> {
        self.roles.get(name)
    }

    pub fn chain(&self, role: Option<&str>) -> &[ModelRef] {
        self.roles
            .get(role.unwrap_or(MAIN_ROLE))
            .map(|spec| spec.models.as_slice())
            .unwrap_or(&[])
    }

    pub fn isolation(&self, role: Option<&str>) -> IsolationPolicy {
        self.roles
            .get(role.unwrap_or(MAIN_ROLE))
            .map(|spec| spec.isolation)
            .unwrap_or(IsolationPolicy::Never)
    }

    pub fn tool_filter(&self, role: &str, registry: &ToolRegistry, depth: u8) -> ToolFilter {
        let mut deny = vec![
            agentloop_core::tool::SUBAGENT_TOOL_NAME.to_owned(),
            agentloop_core::tool::VERIFIER_TOOL_NAME.to_owned(),
            agentloop_core::tool::WORKFLOW_TOOL_NAME.to_owned(),
            "AskUserQuestion".to_owned(),
        ];
        let Some(spec) = self.roles.get(role) else {
            return ToolFilter {
                allow: Vec::new(),
                deny,
            };
        };
        if depth < spec.max_depth {
            deny.retain(|name| {
                name != agentloop_core::tool::SUBAGENT_TOOL_NAME
                    && name != agentloop_core::tool::VERIFIER_TOOL_NAME
                    && name != agentloop_core::tool::WORKFLOW_TOOL_NAME
            });
        }
        let allow = match &spec.tools {
            RoleToolProfile::Full => Vec::new(),
            RoleToolProfile::ReadOnly => registry
                .read_only_names()
                .into_iter()
                .filter(|name| !deny.contains(name))
                .collect(),
            RoleToolProfile::Allow(list) => list
                .iter()
                .filter(|name| !deny.contains(name))
                .cloned()
                .collect(),
        };
        ToolFilter { allow, deny }
    }

    pub fn spawnable(&self) -> Vec<(String, String)> {
        self.roles
            .values()
            .filter(|spec| spec.name != MAIN_ROLE)
            .map(|spec| {
                let access = match &spec.tools {
                    RoleToolProfile::ReadOnly => "read-only tools",
                    RoleToolProfile::Full => "full tool access",
                    RoleToolProfile::Allow(_) => "restricted tool set",
                };
                let model = spec
                    .models
                    .first()
                    .map(|model| model.0.clone())
                    .unwrap_or_else(|| "inherits session model".to_owned());
                (spec.name.clone(), format!("{model}, {access}"))
            })
            .collect()
    }
}

pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    name.len() <= 32
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn builtin_roles() -> Vec<RoleSpec> {
    vec![
        RoleSpec {
            name: "searcher".to_owned(),
            prompt: Some(
                "You are a read-only research subagent. Decide what would answer the \
                 question, search broadly, then read the exact lines that confirm it. \
                 Report only what the code shows — absolute file paths and line numbers — \
                 in a token-efficient brief. You cannot modify anything or ask the user \
                 questions."
                    .to_owned(),
            ),
            ..RoleSpec::new("searcher")
        },
        RoleSpec {
            name: "worker".to_owned(),
            tools: RoleToolProfile::Full,
            prompt: Some(
                "You are an implementation subagent owning one self-contained task. Read \
                 the surrounding code first and state your approach in one line, then make \
                 the smallest change that fully solves the task. Verify it (build/tests \
                 where applicable) and report what changed, how you verified it, and any \
                 assumptions. You cannot ask the user questions."
                    .to_owned(),
            ),
            max_parallel: 3,
            ..RoleSpec::new("worker")
        },
        RoleSpec {
            name: "reviewer".to_owned(),
            prompt: Some(
                "You are a read-only review subagent. Check each stated criterion against \
                 the actual code, reason about edge cases and failure modes the change may \
                 have missed, cite file:line for every finding, and rank by severity \
                 (blocker/major/minor). Do not propose fixes unless asked.\n\n\
                 Output contract: prefix every finding by severity — a required fix gets no \
                 prefix, `Critical:` marks something that risks correctness or data, `Nit:` \
                 marks a stylistic or minor concern, `Optional:` marks a suggestion the author \
                 may reasonably skip. Order findings by leverage: the finding that most changes \
                 the reader's decision comes first, cosmetic nits come last. Lead with what \
                 matters — a few high-conviction findings beat a long list, and burying a real \
                 issue under a pile of nits is worse than omitting the nits. When you flag a \
                 structural problem, propose the specific move (extract a helper, replace a \
                 conditional with a dispatch table), not just the complaint."
                    .to_owned(),
            ),
            ..RoleSpec::new("reviewer")
        },
        RoleSpec {
            name: VERIFIER_ROLE.to_owned(),
            tools: RoleToolProfile::Allow(vec![
                "Read".to_owned(),
                "Glob".to_owned(),
                "Grep".to_owned(),
                agentloop_core::tool::SUBMIT_VERDICT_TOOL_NAME.to_owned(),
            ]),
            max_depth: 0,
            prompt: Some(
                "You are an independent verifier. You did not produce this work and have no \
                 access to how it was produced — assess only whether the listed artifacts \
                 satisfy the rubric you were given. Read the artifacts, check each rubric \
                 criterion against what they actually show, and call SubmitVerdict exactly \
                 once. Do not speculate about intent, process, or anything outside the \
                 artifacts and the rubric."
                    .to_owned(),
            ),
            ..RoleSpec::new(VERIFIER_ROLE)
        },
        RoleSpec {
            name: MAIN_ROLE.to_owned(),
            tools: RoleToolProfile::Full,
            ..RoleSpec::new(MAIN_ROLE)
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_present_and_main_not_spawnable() {
        let registry = RoleRegistry::with_defaults(Vec::new()).expect("defaults build");
        for name in ["searcher", "worker", "reviewer", MAIN_ROLE] {
            assert!(registry.get(name).is_some(), "{name} missing");
        }
        assert!(
            registry
                .spawnable()
                .iter()
                .all(|(name, _)| name != MAIN_ROLE)
        );
    }

    #[test]
    fn verifier_is_spawnable_but_cannot_spawn_further_or_run_shell() {
        let registry = RoleRegistry::with_defaults(Vec::new()).expect("defaults build");
        let verifier = registry.get(VERIFIER_ROLE).expect("verifier role");
        assert_eq!(
            verifier.max_depth, 0,
            "verifiers do not spawn further subagents"
        );
        assert!(
            registry
                .spawnable()
                .iter()
                .any(|(name, _)| name == VERIFIER_ROLE),
            "verifier must be spawnable via the Verify tool"
        );
        match &verifier.tools {
            RoleToolProfile::Allow(tools) => {
                assert!(tools.iter().any(|t| t == "SubmitVerdict"));
                assert!(
                    !tools
                        .iter()
                        .any(|t| t == "Bash" || t == "Write" || t == "Edit")
                );
            }
            other => panic!("expected an explicit allowlist, got {other:?}"),
        }
    }

    #[test]
    fn tool_filter_denies_verify_beyond_max_depth() {
        let registry = RoleRegistry::with_defaults(Vec::new()).expect("defaults build");
        let tools = ToolRegistry::new();
        let at_root = registry.tool_filter("worker", &tools, 0);
        assert!(at_root.permits(agentloop_core::tool::VERIFIER_TOOL_NAME));
        let at_max_depth = registry.tool_filter("worker", &tools, 1);
        assert!(!at_max_depth.permits(agentloop_core::tool::VERIFIER_TOOL_NAME));
    }

    #[test]
    fn user_specs_override_builtins_and_clamp() {
        let user = vec![RoleSpec {
            max_parallel: 99,
            max_depth: 9,
            models: vec![ModelRef::from("mock/a")],
            ..RoleSpec::new("worker")
        }];
        let registry = RoleRegistry::with_defaults(user).expect("builds");
        let worker = registry.get("worker").expect("worker");
        assert_eq!(worker.max_parallel, 8);
        assert_eq!(worker.max_depth, 3);
        assert_eq!(registry.chain(Some("worker")), &[ModelRef::from("mock/a")]);
    }

    #[test]
    fn invalid_and_duplicate_names_reject() {
        assert!(matches!(
            RoleRegistry::with_defaults(vec![RoleSpec::new("Bad Name")]),
            Err(RoleError::InvalidName(_))
        ));
        assert!(matches!(
            RoleRegistry::with_defaults(vec![RoleSpec::new("dup"), RoleSpec::new("dup")]),
            Err(RoleError::Duplicate(_))
        ));
    }

    #[test]
    fn models_only_overlay_keeps_builtin_prompt() {
        let user = vec![RoleSpec {
            models: vec![ModelRef::from("deepseek/deepseek-v4-flash")],
            ..RoleSpec::new("searcher")
        }];
        let registry = RoleRegistry::with_defaults(user).expect("builds");
        let searcher = registry.get("searcher").expect("searcher");
        assert_eq!(searcher.models[0].0, "deepseek/deepseek-v4-flash");
        assert!(
            searcher
                .prompt
                .as_deref()
                .is_some_and(|p| p.contains("read-only research")),
            "builtin searcher prompt must survive a models-only overlay"
        );
    }

    #[test]
    fn spawnable_summary_shows_role_model() {
        let user = vec![
            RoleSpec {
                models: vec![ModelRef::from("deepseek/deepseek-v4-flash")],
                ..RoleSpec::new("searcher")
            },
            RoleSpec {
                models: vec![ModelRef::from("deepseek/deepseek-v4-pro")],
                tools: RoleToolProfile::Full,
                ..RoleSpec::new("worker")
            },
        ];
        let registry = RoleRegistry::with_defaults(user).expect("builds");
        let summary = |name: &str| {
            registry
                .spawnable()
                .into_iter()
                .find(|(role, _)| role == name)
                .map(|(_, text)| text)
                .unwrap_or_default()
        };
        assert!(summary("searcher").contains("deepseek/deepseek-v4-flash"));
        let worker = summary("worker");
        assert!(worker.contains("deepseek/deepseek-v4-pro"));
        assert!(worker.contains("full tool access"));
        assert!(summary("reviewer").contains("inherits session model"));
    }

    #[test]
    fn main_chain_feeds_failover() {
        let user = vec![RoleSpec {
            models: vec![ModelRef::from("a/x"), ModelRef::from("b/y")],
            ..RoleSpec::new(MAIN_ROLE.to_owned())
        }];
        let registry = RoleRegistry::with_defaults(user).expect("builds");
        assert_eq!(registry.chain(None).len(), 2);
    }
}

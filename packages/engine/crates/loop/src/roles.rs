//! Role definitions for multi-agent orchestration.
//!
//! A role names a job (`searcher`, `worker`, `reviewer`, …) and carries an
//! ordered model-preference chain, a tool profile, and spawn limits. The
//! interactive session uses the reserved `main` role's chain for mid-turn
//! failover; subagents spawned by the Task tool get their role's chain,
//! filtered tools, and prompt.

use agentloop_contracts::{IsolationPolicy, ModelRef};
use agentloop_core::{ToolFilter, ToolRegistry};

/// Reserved role driving the interactive session; never spawnable.
pub const MAIN_ROLE: &str = "main";

/// Built-in independent-verifier role, spawned by the `Verify` tool
/// (`agentloop_core::tool::VERIFIER_TOOL_NAME`) — "maker is never the
/// grader". Tighter than `reviewer`: no `Bash`/`Write`/`Edit`, and it cannot
/// spawn further subagents.
pub const VERIFIER_ROLE: &str = "verifier";

/// How a role's tool set is derived from the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleToolProfile {
    /// Every registry tool whose descriptor says `read_only`.
    ReadOnly,
    /// Every registry tool.
    Full,
    /// An explicit allow-list of tool names.
    Allow(Vec<String>),
}

/// One role definition (user-configured or built-in).
#[derive(Debug, Clone, PartialEq)]
pub struct RoleSpec {
    /// Role name: `^[a-z0-9][a-z0-9_-]{0,31}$`.
    pub name: String,
    /// Ordered model preference chain; empty = inherit the spawning
    /// session's effective model.
    pub models: Vec<ModelRef>,
    /// Which tools the role may use.
    pub tools: RoleToolProfile,
    /// System-prompt addition delivered via `TurnOptions.system_append`.
    pub prompt: Option<String>,
    /// Distribute parallel spawns across the chain (round-robin).
    pub split: bool,
    /// Concurrent subagents of this role per batch (clamped 1..=8).
    pub max_parallel: usize,
    /// Spawn-tree depth this role may create below itself (clamped 0..=3).
    pub max_depth: u8,
    /// Whether a root session serving this role runs in an isolated workspace.
    /// Only consulted for root sessions (depth 0); subagents inherit the
    /// parent's working directory. Defaults to [`IsolationPolicy::Never`], so
    /// isolation is opt-in.
    pub isolation: IsolationPolicy,
}

impl RoleSpec {
    /// A role with conservative defaults: inherit model, read-only tools, no
    /// isolation.
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

/// Why a role set was rejected.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RoleError {
    /// The name violates `^[a-z0-9][a-z0-9_-]{0,31}$`.
    #[error("role `{0}` has an invalid name (use a-z, 0-9, -, _; max 32 chars)")]
    InvalidName(String),
    /// The same role appears twice.
    #[error("role `{0}` is declared more than once")]
    Duplicate(String),
}

/// Built-in defaults overlaid by user specs; the lookup used by the Task
/// tool and by chain resolution.
#[derive(Debug, Default)]
pub struct RoleRegistry {
    roles: std::collections::BTreeMap<String, RoleSpec>,
}

impl RoleRegistry {
    /// Built-ins (`searcher`, `worker`, `reviewer`, `main`) overlaid by
    /// `user` specs (same-name user specs replace built-ins).
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
            spec.max_parallel = spec.max_parallel.clamp(1, 8);
            spec.max_depth = spec.max_depth.min(3);
            registry.roles.insert(spec.name.clone(), spec);
        }
        Ok(registry)
    }

    /// Look up a role by name.
    pub fn get(&self, name: &str) -> Option<&RoleSpec> {
        self.roles.get(name)
    }

    /// The fallback chain for a session serving `role` (`None` = main).
    pub fn chain(&self, role: Option<&str>) -> &[ModelRef] {
        self.roles
            .get(role.unwrap_or(MAIN_ROLE))
            .map(|spec| spec.models.as_slice())
            .unwrap_or(&[])
    }

    /// The isolation policy for a session serving `role` (`None` = main).
    /// Unknown roles fall back to [`IsolationPolicy::Never`].
    pub fn isolation(&self, role: Option<&str>) -> IsolationPolicy {
        self.roles
            .get(role.unwrap_or(MAIN_ROLE))
            .map(|spec| spec.isolation)
            .unwrap_or(IsolationPolicy::Never)
    }

    /// The tool filter for a session serving `role` at spawn `depth`.
    /// Subagents never get `AskUserQuestion` (they have no user) and lose
    /// the `Task`/`Verify`/`RunWorkflow` tools once they reach their role's
    /// `max_depth` — `Verify` and `RunWorkflow` also spawn children (a
    /// constrained `verifier` subagent, and each workflow step
    /// respectively), so both are gated by the same depth budget as `Agent`.
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

    /// Roles the Task tool may spawn: everything except `main`, as
    /// `(name, one-line summary)` pairs for the tool description. The summary
    /// leads with the role's model so the orchestrator can see that different
    /// roles run different models (e.g. a fast model for research, a strong one
    /// for implementation) and route by task accordingly.
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

/// Whether `name` is a legal role name: `^[a-z0-9][a-z0-9_-]{0,31}$`.
/// Exposed so clients can pre-validate config entries with the same rule
/// instead of failing the whole engine build on one bad role.
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
        // worker's max_depth is 1 by default: permitted at depth 0, denied at depth 1.
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

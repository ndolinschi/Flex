
use super::common::require_service;
use super::memory::{memory_dir, purge_expired_memories};
use super::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptReviewFindingDto {
    pub quote: String,
    pub severity: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptReviewDto {
    pub summary: String,
    pub findings: Vec<PromptReviewFindingDto>,
    #[serde(default)]
    pub questions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptReviewAnswerDto {
    pub question: String,
    pub answer: String,
}

#[derive(Debug, Deserialize)]
struct PromptReviewModelPayload {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    findings: Vec<PromptReviewFindingDto>,
    #[serde(default)]
    questions: Vec<String>,
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn review_prompt(
    state: State<'_, AppState>,
    session_id: String,
    prompt_text: String,
    answers: Option<Vec<PromptReviewAnswerDto>>,
) -> DesktopResult<PromptReviewDto> {
    let text = prompt_text.trim().to_string();
    if text.is_empty() {
        return Err(DesktopError::Message("prompt is empty".into()));
    }

    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let meta = service.session_meta(&id).await?;
    let model = meta
        .model
        .ok_or_else(|| DesktopError::Message("session has no model set".into()))?;

    let registry = service.provider_registry();
    let (provider, model_id) = registry
        .resolve(&model)
        .ok_or_else(|| DesktopError::Message(format!("no provider for model {model}")))?;

    let truncated: String = text.chars().take(12_000).collect();
    let system = "You are a ruthless prompt coach for coding agents (grill the draft). \
        Critique the user prompt. Look for: typos and wrong words, vague goals, \
        missing constraints/success criteria, empty or useless context, bloated laundry lists, \
        and missing canonical examples when the task is complex. \
        If critical facts are missing, ask up to 3 short clarifying questions. \
        Reply with JSON only (no markdown fences), shape: \
        {\"summary\":\"one sentence\",\"findings\":[{\"quote\":\"exact substring from the prompt\", \
        \"severity\":\"error\"|\"warn\"|\"info\",\"message\":\"what is wrong\", \
        \"fix\":\"optional concrete rewrite for that quote\"}], \
        \"questions\":[\"optional clarifying question\", ...]}. \
        quote MUST be copied verbatim from the prompt (short span). \
        Use severity error for typos/broken instructions, warn for weak/missing context, \
        info for polish. Prefer 3–8 high-signal findings; skip praise. \
        If the user already answered questions, fold those answers into your critique \
        and only ask new questions when still blocked."
        .to_string();

    let mut user_body =
        format!("Critique this agent prompt:\n\n----- PROMPT -----\n{truncated}\n----- END -----");
    if let Some(ans) = answers.as_ref() {
        let answered: Vec<_> = ans.iter().filter(|a| !a.answer.trim().is_empty()).collect();
        if !answered.is_empty() {
            user_body.push_str("\n\n----- ANSWERS TO YOUR QUESTIONS -----\n");
            for a in answered {
                user_body.push_str(&format!("Q: {}\nA: {}\n\n", a.question, a.answer.trim()));
            }
            user_body.push_str("----- END ANSWERS -----\n");
        }
    }

    let mut request = ChatRequest::new(model_id, vec![Message::user(user_body)]);
    request.system = Some(system);
    request.max_tokens = Some(2048);

    let cancel = CancellationToken::new();
    let mut stream = provider
        .stream_chat(request, cancel)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;

    let mut raw = String::new();
    while let Some(event) = stream.next().await {
        match event.map_err(|e| DesktopError::Message(e.to_string()))? {
            ProviderStreamEvent::MarkdownDelta { text: delta } => {
                raw.push_str(&delta);
            }
            ProviderStreamEvent::MessageEnd { .. } => break,
            _ => {}
        }
    }

    let json_slice = extract_json_object(&raw)
        .ok_or_else(|| DesktopError::Message("prompt review returned no JSON".into()))?;
    let parsed: PromptReviewModelPayload = serde_json::from_str(json_slice)
        .map_err(|e| DesktopError::Message(format!("prompt review JSON parse: {e}")))?;

    let findings: Vec<PromptReviewFindingDto> = parsed
        .findings
        .into_iter()
        .filter(|f| !f.quote.trim().is_empty() && !f.message.trim().is_empty())
        .map(|mut f| {
            let sev = f.severity.to_ascii_lowercase();
            f.severity = match sev.as_str() {
                "error" | "warn" | "info" => sev,
                "warning" => "warn".into(),
                _ => "warn".into(),
            };
            f
        })
        .collect();

    let questions: Vec<String> = parsed
        .questions
        .into_iter()
        .map(|q| q.trim().to_string())
        .filter(|q| !q.is_empty())
        .take(3)
        .collect();

    Ok(PromptReviewDto {
        summary: if parsed.summary.trim().is_empty() {
            if findings.is_empty() && questions.is_empty() {
                "Looks solid — no major issues found.".into()
            } else {
                let q = if questions.is_empty() {
                    String::new()
                } else {
                    format!(", {} question(s)", questions.len())
                };
                format!("{} issue(s){} to tighten.", findings.len(), q)
            }
        } else {
            parsed.summary.trim().to_string()
        },
        findings,
        questions,
    })
}

pub(crate) fn extract_json_object(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return Some(&trimmed[start..=end]);
            }
        }
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptAttachment {
    pub path: String,
    pub kind: String,
    pub name: Option<String>,
    pub media_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCommandInput {
    pub session_id: String,
    pub text: String,
    pub model: Option<String>,
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub attachments: Vec<PromptAttachment>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub composer_mode: Option<String>,
}

pub(crate) fn parse_effort(raw: Option<&str>) -> Option<Effort> {
    match raw? {
        "low" => Some(Effort::Low),
        "medium" => Some(Effort::Medium),
        "high" => Some(Effort::High),
        "xhigh" => Some(Effort::XHigh),
        "max" => Some(Effort::Max),
        _ => None,
    }
}

pub(crate) fn parse_permission_mode(raw: Option<&str>) -> Option<PermissionMode> {
    match raw? {
        "default" => Some(PermissionMode::Default),
        "accept_edits" | "acceptEdits" => Some(PermissionMode::AcceptEdits),
        "plan" => Some(PermissionMode::Plan),
        "dont_ask" | "dontAsk" => Some(PermissionMode::DontAsk),
        "bypass_permissions" | "bypassPermissions" => Some(PermissionMode::BypassPermissions),
        _ => None,
    }
}

pub(crate) fn guess_media_type(path: &str, kind: &str) -> String {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match (kind, ext.as_str()) {
        ("image", "png") => "image/png".into(),
        ("image", "jpg" | "jpeg") => "image/jpeg".into(),
        ("image", "gif") => "image/gif".into(),
        ("image", "webp") => "image/webp".into(),
        ("image", _) => "image/png".into(),
        (_, "pdf") => "application/pdf".into(),
        (_, "md" | "markdown") => "text/markdown".into(),
        (_, "json") => "application/json".into(),
        (_, "xml" | "svg") => "application/xml".into(),
        (_, "html" | "htm") => "text/html".into(),
        (_, "css") => "text/css".into(),
        (_, "yaml" | "yml") => "text/yaml".into(),
        (_, "toml") => "text/toml".into(),
        (
            _,
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "rs" | "txt" | "py" | "pyi" | "go"
            | "java" | "kt" | "kts" | "c" | "cc" | "cpp" | "cxx" | "h" | "hh" | "hpp" | "cs" | "rb"
            | "php" | "swift" | "sh" | "bash" | "zsh" | "fish" | "ps1" | "sql" | "r" | "lua"
            | "vim" | "el" | "clj" | "scala" | "rsx" | "svelte" | "vue" | "astro" | "gradle"
            | "dockerfile" | "makefile" | "cmake" | "ini" | "cfg" | "conf" | "env" | "gitignore"
            | "dockerignore" | "editorconfig" | "lock",
        ) => "text/plain".into(),
        _ => "application/octet-stream".into(),
    }
}

pub(crate) fn build_prompt_input(input: &PromptCommandInput) -> PromptInput {
    let mut parts = Vec::new();
    let text = input.text.trim();
    if !text.is_empty() {
        parts.push(ContentBlock::markdown(text));
    }
    for att in &input.attachments {
        let path = PathBuf::from(&att.path);
        let name = att
            .name
            .clone()
            .or_else(|| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.trim_end_matches('/').to_owned())
            })
            .unwrap_or_else(|| "attachment".into());
        if att.kind == "directory" {
            let display = att.path.trim_end_matches('/');
            parts.push(ContentBlock::markdown(format!(
                "Referenced directory: `{display}/`"
            )));
            continue;
        }
        let media_type = att
            .media_type
            .clone()
            .unwrap_or_else(|| guess_media_type(&att.path, &att.kind));
        let data = BlobSource::Path { path };
        if att.kind == "image" {
            parts.push(ContentBlock::Image { media_type, data });
        } else {
            parts.push(ContentBlock::File {
                name,
                media_type,
                data,
            });
        }
    }
    if parts.is_empty() {
        return PromptInput::text("");
    }
    PromptInput {
        parts,
        command: None,
    }
}

const PROJECT_MEMORY_PROMPT_BUDGET_CHARS: usize = 4_000;

const PROJECT_INSTRUCTIONS_BUDGET_CHARS: usize = 12_000;

const DEFAULT_DELEGATION_RULES_CORE: &str = "\
# Delegation rules (system defaults — project delegation.md overrides)

Use `Agent` (subagent) for:
- Any task that touches more than 5 files
- Work that benefits from isolation (risky refactors, large rewrites)
- Parallel independent sub-tasks (spawn multiple workers)

Use `SwitchMode` when available and the user's intent clearly matches a different mode:
- Planning without execution → propose `plan`
- Debugging a concrete failure → propose `debug`
- Pure question with no edits → propose `ask`
- Do NOT switch modes mid-task without a clear signal

When in doubt, self-execute rather than delegate.";

const AUTO_MODE_ROUTING_PROMPT: &str = "\
# Auto mode

This turn started with a **low-cost model and low reasoning effort**. \
After reading the task, call `SetRouting` ONCE (early in the turn, before doing \
significant work) when the default routing is insufficient:

```
SetRouting({
  model: \"<provider/model from the allowed list>\",  // optional
  effort: \"low|medium|high|xhigh|max\",              // optional
  reason: \"one-sentence justification\"
})
```

Routing guidelines:
- **Low (default)**: quick answers, single-file reads/lookups, tiny edits — stay here.
- **Medium**: multi-step tasks, moderate code changes — escalate model and/or effort.
- **High**: complex refactors, multi-file features, deep reasoning — escalate further.
- Do NOT call `SetRouting` more than once per turn; the first call wins.
- Prefer staying low-cost when the task is only reading or answering.
- The `SetRouting` tool description lists the exact model ids for this session.

You also have `SwitchMode(mode, reason)` — propose switching to plan / ask / debug / agent. \
The user sees a brief veto countdown.

Apply the delegation rules above. Prefer self-execution for small tasks; \
delegate large or parallel work via `Agent`.";

const AUTO_MODE_MESSAGING_PROMPT: &str = "\
## Peer coordination

You also have:
- `GetActiveAgents` — see which other agents are running and what they are working on
- `SendMessage` / `GetMessages` — exchange notes with a peer about a shared file

Use `GetActiveAgents` before editing a path another agent may be modifying.";

const FLEX_ORCHESTRATOR_PROMPT: &str = "\
You are an orchestrator. First classify the task:
- SIMPLE (single-file change, question, quick fix): do it yourself directly, no subagents.
- COMPLEX (multi-file feature, refactor, \"build X\"): orchestrate as below.

PLAN: if you are a top-tier model, draft the plan yourself; otherwise spawn \
Agent(role=planner, model=<top tier>) with the full task. The planner may \
spawn its own read-only context gatherers.

REVIEW: send the plan to Verify (or Agent role=plan-reviewer) using a \
DIFFERENT model than the planner, passing ONLY the task statement plus the \
plan text — nothing else. If REJECTED: revise with the planner, addressing \
every numbered objection. Hard limit: 3 revision cycles. After the 3rd \
rejection, stop and present both the plan and the objections to the user \
for a decision — do not keep revising past that point.

EXECUTE: once the plan is APPROVED, split it into independent steps and \
spawn flex-worker agents (each gets an isolated worktree, merged back \
automatically) with COMPLETE, self-contained prompts — the step, the \
relevant file paths, and the verification commands to run. Run independent \
steps in parallel, up to 8 at a time.

MERGE/VERIFY: after workers finish, review the integration results, run the \
project's verification commands, and summarize what changed.

Model tiers: pick the top-tier planner from the models available to you by \
name (opus/sol/terra/o1-class names are top tier; sonnet/grok/gpt-class names \
are middle tier). Always use the full `provider/model` id from your model \
list when overriding a subagent's model.

You run with DontAsk permissions, and every subagent you spawn inherits \
that: never leave a permission ask pending, and never block waiting on one.";

const FLEX_PLAN_PROMPT: &str = "\
Plan mode: INVESTIGATE before you plan. First use your read-only tools \
(SearchCode/FindSymbol, Read, and RepoMap when available) to find the actual \
files, symbols, and current behavior relevant to the task. Ground the plan in \
what you found — cite concrete file paths (with line ranges) and quote the real \
current code or strings you intend to change. Do NOT hand back a generic \
checklist of investigative steps (e.g. \"locate the code\", \"identify the \
logic\") — that investigation is your job to do now, before answering. Present \
a concrete, grounded plan only after you have actually explored the code.";

const DEBUG_MODE_PROMPT: &str = "\
# Debug mode

You are debugging — same tools as Agent mode, different discipline. Do not \
guess-and-patch. Follow this loop and narrate which step you are on:

1. **Gather evidence** — collect the maximum useful signal before editing: \
error messages, stack traces, failing tests, logs, network/console output, \
relevant source, and (when available) Browser/Computer tools for UI bugs.
2. **Reproduce first** — make the failure happen on demand. Explicitly confirm \
\"reproduced: yes\" (or \"cannot reproduce yet\" with what you still need). \
Do not claim a fix until reproduction is solid.
3. **Localize** — narrow WHERE it fails (file/symbol/layer). Prefer bisection \
and reading over speculative rewrites.
4. **Probe** — inject the SMALLEST temporary instrumentation that teaches you \
something: logs, assertions, counters, feature flags. Mark every probe so it \
is greppable and removable:
   - Prefer comments/tags containing exactly `AGENT-DEBUG` (e.g. \
`// AGENT-DEBUG`, `# AGENT-DEBUG`, `{/* AGENT-DEBUG */}`).
   - Never commit or leave probes in the final answer.
5. **Rerun** — re-run the failing path (tests, CLI, Browser flow, app). Keep \
probes only while they earn their keep.
6. **Fix the root cause** — one clear fix grounded in evidence from steps 1–5.
7. **Clean up** — when verification passes, REMOVE every `AGENT-DEBUG` probe \
and any other temporary scaffolding. Grep the tree for `AGENT-DEBUG` before \
you stop. The delivered result must be the real fix only — no leftover debug \
noise.
8. **Verify clean** — re-run once more after cleanup so you did not remove \
something load-bearing.

## UI / frontend / desktop bugs

When Browser tools are registered (`BrowserNavigate`, `BrowserScreenshot`, \
`BrowserEval`, `BrowserClick`, `BrowserConsole`, `BrowserOpenDevtools`), prefer \
them for live page failures — console dumps and DOM/source via eval often beat \
guessing from static files alone.

When Computer tools are registered (`ComputerScreenshot`, `ComputerMove`, \
`ComputerClick`, `ComputerType`, `ComputerOpenApp`), use them for OS/desktop \
repros (open the app, drive UI, capture state). Respect permission prompts.

## What NOT to do

- Do not ship speculative multi-file refactors while the bug is unreproduced.
- Do not leave `AGENT-DEBUG` (or equivalent) instrumentation in the final diff.
- Do not treat a single green run with probes still present as done.
- IDE-native debuggers (e.g. attaching IntelliJ to a running process) are out \
of scope for this mode — stick to in-repo probes + tools above.";

const DEBUG_VISION_YES: &str = "\
## Vision: ENABLED for this model

Screenshots from Browser/Computer tools (and user image attachments) are \
first-class evidence — use them when a visual or layout bug is suspected. \
Still pair images with console/DOM/text so the fix is grounded in code.";

const DEBUG_VISION_NO: &str = "\
## Vision: DISABLED for this model

Do NOT rely on screenshots as primary evidence — this model cannot see images. \
Prefer text channels instead:
- `BrowserConsole` / console dumps
- `BrowserEval` to read DOM, computed styles, React/Vue props, error objects
- page HTML/source excerpts, network/log files, stack traces
- describe UI state in words after probing the DOM
If a screenshot tool returns a path, treat it as opaque unless the user can \
interpret it; extract facts via eval/logs instead.";

const DEBUG_VISION_UNKNOWN: &str = "\
## Vision: UNKNOWN for this model

Prefer text evidence (console, DOM via eval, logs, source). Use screenshots \
only as a supplement — never as the sole proof of a visual bug.";

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn prompt(
    state: State<'_, AppState>,
    input: PromptCommandInput,
) -> DesktopResult<TurnSummary> {
    let result = prompt_inner(state, input).await;
    if let Err(err) = &result {
        let msg = err.to_string();
        if msg.contains("already in progress") {
            tracing::debug!(error = %msg, "prompt rejected: turn in progress");
        } else {
            tracing::error!(error = %msg, "prompt failed");
        }
    }
    result
}

pub(crate) fn session_cwd_notice(cwd: &std::path::Path) -> String {
    format!(
        "Session working directory: {}. This is the ONLY working directory for \
         this session: use it for all relative paths, project context, and any \
         question about where the user's project lives. It authoritatively \
         overrides the 'Working directory at engine startup' line stated earlier \
         in this prompt, which reflects a different session (or none) and MUST \
         be ignored for this turn.",
        cwd.display()
    )
}

pub(crate) async fn resolve_model_vision(service: &EngineService, model: &str) -> Option<bool> {
    let model_ref = ModelRef(model.to_owned());
    let (provider, model_id) = service.provider_registry().resolve(&model_ref)?;
    let models = provider.list_models().await.ok()?;
    models
        .into_iter()
        .find(|m| m.id == model_id)
        .map(|m| m.vision)
}

pub(crate) fn append_system(existing: Option<String>, fragment: &str) -> Option<String> {
    Some(match existing {
        Some(prior) => format!("{prior}\n\n{fragment}"),
        None => fragment.to_owned(),
    })
}

fn join_models(models: &[String]) -> String {
    let cleaned: Vec<&str> = models
        .iter()
        .map(|m| m.trim())
        .filter(|m| !m.is_empty())
        .collect();
    if cleaned.is_empty() {
        "_(none configured)_".to_owned()
    } else {
        cleaned.join(", ")
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub(crate) async fn prompt_inner(
    state: State<'_, AppState>,
    input: PromptCommandInput,
) -> DesktopResult<TurnSummary> {
    let service = require_service(&state).await?;
    let id = SessionId::from(input.session_id.clone());
    let meta = service.session_meta(&id).await.ok();
    let cwd_notice = meta.as_ref().map(|meta| session_cwd_notice(&meta.cwd));
    if let Ok(dir) = memory_dir() {
        purge_expired_memories(&dir);
    }
    if let Some(meta) = meta.as_ref() {
        purge_expired_memories(&meta.cwd.join(".agent").join("memory"));
    }
    let project_memory = meta.as_ref().and_then(|meta| {
        agentloop_prompts::load_memory_section(&agentloop_prompts::MemoryConfig {
            dir: Some(meta.cwd.join(".agent").join("memory")),
            budget_chars: PROJECT_MEMORY_PROMPT_BUDGET_CHARS,
        })
    });
    let project_instructions = meta.as_ref().and_then(|meta| {
        let loaded = agentloop_prompts::load_project_instructions(
            &meta.cwd,
            PROJECT_INSTRUCTIONS_BUDGET_CHARS,
        );
        agentloop_prompts::format_project_instructions_section(&loaded)
    });
    let is_auto_model = input.model.as_deref() == Some("auto");
    let is_auto_mode = input.composer_mode.as_deref() == Some("auto") || is_auto_model;
    let mut system_append = match (cwd_notice, project_memory) {
        (Some(a), Some(b)) => Some(format!("{a}\n\n{b}")),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    if let Some(instr) = project_instructions {
        system_append = append_system(system_append, &instr);
    }
    if input.composer_mode.as_deref() == Some("flex") {
        system_append = append_system(system_append, FLEX_ORCHESTRATOR_PROMPT);
    }
    if input.composer_mode.as_deref() == Some("plan") {
        system_append = append_system(system_append, FLEX_PLAN_PROMPT);
    }
    if is_auto_mode {
        let (delegation_fragment, messaging_on, cost_lists) = {
            let cfg = state.config.lock().await;
            let r = cfg.prefs.plugins.delegation_rules.trim().to_owned();
            let delegation = if r.is_empty() {
                DEFAULT_DELEGATION_RULES_CORE.to_owned()
            } else {
                r
            };
            let lists = format!(
                "## Configured cost-tier models\n\
                 - **low**: {}\n\
                 - **medium**: {}\n\
                 - **high**: {}\n\
                 - **cost mode**: `{}`",
                join_models(&cfg.prefs.plugins.cost_models_low),
                join_models(&cfg.prefs.plugins.cost_models_medium),
                join_models(&cfg.prefs.plugins.cost_models_high),
                cfg.prefs.plugins.cost_mode,
            );
            (delegation, cfg.prefs.plugins.messaging, lists)
        };
        system_append = append_system(system_append, &delegation_fragment);
        system_append = append_system(system_append, AUTO_MODE_ROUTING_PROMPT);
        system_append = append_system(system_append, &cost_lists);
        if messaging_on {
            system_append = append_system(system_append, AUTO_MODE_MESSAGING_PROMPT);
        }
    }
    if input.composer_mode.as_deref() == Some("debug") {
        system_append = append_system(system_append, DEBUG_MODE_PROMPT);
        let model_key = input.model.as_deref().or_else(|| {
            meta.as_ref()
                .and_then(|m| m.model.as_ref().map(|r| r.0.as_str()))
        });
        let vision = match model_key {
            Some(key) => resolve_model_vision(&service, key).await,
            None => None,
        };
        let vision_frag = match vision {
            Some(true) => DEBUG_VISION_YES,
            Some(false) => DEBUG_VISION_NO,
            None => DEBUG_VISION_UNKNOWN,
        };
        system_append = append_system(system_append, vision_frag);
    }
    let resolved_model = if is_auto_model {
        let cfg = state.config.lock().await;
        cfg.prefs
            .plugins
            .cost_models_low
            .first()
            .cloned()
            .filter(|m| !m.is_empty())
            .or_else(|| {
                cfg.prefs
                    .plugins
                    .auto_mode_router_model
                    .clone()
                    .filter(|m| !m.is_empty())
            })
    } else {
        input.model.as_deref().map(str::to_owned)
    };
    let effort = if is_auto_mode && input.effort.is_none() {
        Some(Effort::Low)
    } else {
        parse_effort(input.effort.as_deref())
    };
    let opts = TurnOptions {
        model: resolved_model.map(ModelRef),
        permission_mode: parse_permission_mode(input.permission_mode.as_deref()),
        system_append,
        effort,
        ..TurnOptions::default()
    };
    let prompt_input = build_prompt_input(&input);
    Ok(service.prompt(&id, prompt_input, opts).await?)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn cancel(state: State<'_, AppState>, session_id: String) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.cancel(&id).await?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundProcessDto {
    pub process_id: String,
    pub command: Option<String>,
    pub running: bool,
    pub started_at_ms: Option<u64>,
    pub exit_code: Option<i32>,
}

impl From<BackgroundEntrySummary> for BackgroundProcessDto {
    fn from(entry: BackgroundEntrySummary) -> Self {
        Self {
            process_id: entry.id,
            command: Some(entry.command),
            running: entry.running,
            started_at_ms: Some(entry.started_at_ms),
            exit_code: entry.exit_code,
        }
    }
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn background_list(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<Vec<BackgroundProcessDto>> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service
        .background_list(&id)
        .into_iter()
        .map(BackgroundProcessDto::from)
        .collect())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn background_kill(
    state: State<'_, AppState>,
    session_id: String,
    process_id: String,
) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let _ = service.background_kill(&id, &process_id).await?;
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn background_demote(
    state: State<'_, AppState>,
    session_id: String,
    call_id: String,
) -> DesktopResult<bool> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.background_demote(&id, &call_id))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RespondPermissionInput {
    pub session_id: String,
    pub request_id: String,
    pub decision: String,
    pub reason: Option<String>,
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn set_turn_permission_mode(
    state: State<'_, AppState>,
    session_id: String,
    mode: Option<String>,
) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let parsed = match mode.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(
            parse_permission_mode(Some(raw))
                .ok_or_else(|| DesktopError::Message(format!("unknown permission mode: {raw}")))?,
        ),
    };
    Ok(service.set_turn_permission_mode(&id, parsed)?)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn respond_permission(
    state: State<'_, AppState>,
    input: RespondPermissionInput,
) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(input.session_id);
    let request_id = PermissionRequestId::from(input.request_id);
    let decision = match input.decision.as_str() {
        "allow_once" | "allowOnce" => PermissionDecision::AllowOnce,
        "allow_always" | "allowAlways" => PermissionDecision::AllowAlways,
        "deny" => PermissionDecision::Deny {
            reason: input.reason,
        },
        other => {
            return Err(DesktopError::Message(format!(
                "unknown permission decision: {other}"
            )));
        }
    };
    Ok(service
        .respond_permission(&id, request_id, decision)
        .await?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RespondQuestionInput {
    pub session_id: String,
    pub request_id: String,
    pub answers: Vec<AnswerDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerDto {
    pub question: String,
    pub selected: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RespondModeSwitchInput {
    pub session_id: String,
    pub id: String,
    pub allow: bool,
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn respond_mode_switch(
    state: State<'_, AppState>,
    input: RespondModeSwitchInput,
) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let session_id = SessionId::from(input.session_id);
    let switch_id = ModeSwitchId::from(input.id);
    Ok(service
        .respond_mode_switch(&session_id, switch_id, input.allow)
        .await?)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn respond_question(
    state: State<'_, AppState>,
    input: RespondQuestionInput,
) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(input.session_id);
    let request_id = QuestionId::from(input.request_id);
    let answers: Vec<Answer> = input
        .answers
        .into_iter()
        .map(|a| Answer {
            question: a.question,
            selected: a.selected,
        })
        .collect();
    Ok(service.respond_question(&id, request_id, answers).await?)
}

#[cfg(test)]
mod prompt_cwd_tests {
    use super::*;

    #[test]
    fn session_cwd_notice_reflects_only_its_own_session() {
        let test_flex = std::path::Path::new("/Users/example/Documents/Projects/TestFlex");
        let test_next = std::path::Path::new("/Users/example/Documents/Apps/TestNext");

        let flex_notice = session_cwd_notice(test_flex);
        let next_notice = session_cwd_notice(test_next);

        assert!(flex_notice.contains("/Users/example/Documents/Projects/TestFlex"));
        assert!(!flex_notice.contains("/Users/example/Documents/Apps/TestNext"));

        assert!(next_notice.contains("/Users/example/Documents/Apps/TestNext"));
        assert!(!next_notice.contains("/Users/example/Documents/Projects/TestFlex"));

        assert_ne!(flex_notice, next_notice);
    }

    #[test]
    fn session_cwd_notice_overrides_the_engine_startup_line() {
        let notice = session_cwd_notice(std::path::Path::new("/repo"));
        assert!(notice.contains("Working directory at engine startup"));
        assert!(notice.to_lowercase().contains("overrides"));
    }
}

# COMPONENTS.md — per-crate catalog

One entry per crate: purpose, key public types/traits, entry-point files. Use this
before grepping. Derived documentation — **AGENTS.md is authoritative** for rules and
contracts. Companions: [ARCHITECTURE.md](ARCHITECTURE.md), [TECHSTACK.md](TECHSTACK.md).

## packages/engine (hub workspace)

### contracts (`agentloop-contracts`) — `crates/contracts/src/`
Pure data model; serde/schemars/uuid only, no tokio, no I/O. Key modules:
`event.rs` (`AgentEvent` — `#[non_exhaustive]`, tag `"kind"` — and the `SessionEvent`
envelope with `seq`/`ts_ms`), `reduce.rs` (`reduce(events) -> Transcript`,
`TranscriptItem`/`TranscriptBlock`), `content.rs` (`ContentBlock`, `Message`, `Role`),
`tool_call.rs` (`ToolCall`, `ToolCallStatus`, `ToolOutput`), `session.rs`
(`SessionMeta`, `TurnSummary`, `StopReason`), `request.rs` (`NewSessionParams`,
`PromptInput`, `TurnOptions`), `permission.rs`, `capability.rs` (`AgentCaps`,
`ProviderCaps`, `ModelRef`), `error.rs` (`EngineError`, `ErrorCode`), `workspace.rs`
(`IsolationPolicy`), `checkpoint.rs`, `hook.rs` (`HookPoint`), `ids.rs`, `branding.rs`
(the one place the product name appears in code).

### core (`agentloop-core`) — `crates/core/src/`
Traits and registries; depends only on `contracts`. `agent.rs` (`Agent`:
`create_session`/`prompt`/`events`/`cancel`/`respond_permission`/`compact`),
`provider.rs` (`Provider`, `ProviderStreamEvent`, `ChatRequest`, `ProviderError`),
`tool.rs` (`Tool`, `ToolContext`, `ToolDescriptor`), `store.rs` (`SessionStore`),
`plugin.rs` (`Plugin`, `PluginRegistry`, `PluginRole`), `hook.rs` (`Hook`),
`workspace.rs` (`Workspaces`), `executor.rs` (`Executor`, `ExecSpec`, `NetworkPolicy`),
`event_sink.rs` (`EventSink`), `registry.rs` (`ToolRegistry`, `ProviderRegistry`).

### engine (`agentloop-engine`) — `crates/engine/src/`
`EngineService` front door — provider-agnostic composition over a prebuilt
`ProviderRegistry`; folds plugins into tools/prompt/roles; wires `RoleRegistry`,
`CommandRegistry`, `SystemPromptAssembler`, `McpManager`, base tools.
`lib.rs` (re-exports), `service.rs` (struct + constructors), `native/` (composition),
`session.rs` / `workspace.rs` / `background.rs` / `turn_api.rs` / `replay.rs` /
`goal.rs` / `verify.rs`, `options.rs` (`EngineConfig`, `OutputVerbosity`),
`error.rs` (`EngineServiceError`), `paths.rs`.

### loop (`agentloop-loop`) — `crates/loop/src/`
`NativeAgent`, the native agent loop. `agent.rs` (`NativeAgent`), `builder.rs`
(`NativeAgentBuilder`, `LoopLimits`), `turn/mod.rs` + `turn/iteration/` (model
streaming, retries, model failover) + `turn/tool_exec/` (`execute_tool_requests`,
`execute_one_call`, intercept paths) + `turn/hooks.rs`, `roles.rs` (`RoleRegistry`, `RoleSpec`),
`subagent.rs`, `permission.rs` (`PermissionPolicy`, `Verdict`), `rules.rs`, `pool.rs`
(bounded tool worker pool), `compaction.rs`, `session_handle.rs` (`SessionHandle` —
append-then-broadcast), `manager.rs` (`ToolCallManager`), `actor.rs`,
`context_budget.rs`, `messages.rs` (`transcript_to_messages`), `deps.rs` (`TurnDeps`).

### executors (`agentloop-executors`) — `crates/executors/src/`
`core::Executor` backends: `local.rs` (`/bin/sh`), `docker.rs`, `ssh.rs` (+rsync),
`container_image.rs` (apptainer/singularity), `remote_fn.rs` (serverless stub),
`run/` (shared process plumbing: foreground/demote/background/io/probe).

### hooks (`agentloop-hooks`) — `crates/hooks/src/`
Built-in lifecycle hooks: `diagnostics.rs` (`DiagnosticsHook`, `CheckSpec`), `format.rs`
(`FormatOnEditHook`, `FormatterSpec`), `injection.rs` (`InjectionScanHook`, `scan_text`
prompt-injection heuristics).

### learning (`agentloop-learning`) — `crates/learning/src/`
`LearningPlugin` (self-learning skills + local memory): `save.rs` (`SkillSave` tool),
`memory.rs` (`MemoryWrite`, `~/.config/agentloop/memory/*.md`), `hook.rs` (Stop-point
reflection).

### verifier (`agentloop-verifier`) — `crates/verifier/src/`
`VerifierPlugin` (independent verifier — "maker is never the grader"): `tools.rs` (`Verify`,
`SubmitVerdict`; `Verify` is loop-intercepted by name, same as `Agent`).

### prompts (`agentloop-prompts`) — `crates/prompts/src/`
System-prompt assembly + slash commands: `assembler.rs` (`SystemPromptAssembler`),
`commands.rs` (`CommandRegistry`), `memory.rs` (`load_memory_section`), `skills.rs`
(`SkillRegistry`). Prompt data lives in `packages/engine/prompts/system/*.md`.

### session (`agentloop-session`) — `crates/session/src/`
`SessionStore` impls: `memory.rs` (`MemoryStore`), `jsonl.rs` (`JsonlStore` —
append-only `.jsonl` per session, `LineRecord::{Meta,Event,Delete,Checkpoint}`).

### tools (`agentloop-tools`) — `crates/tools/src/`
Base tool set: `bash/` (`BashTool`, takes `Arc<dyn Executor>`), `fs/` (Read/Write/Edit
+ helpers/html), `glob.rs`, `grep.rs`, `web_fetch.rs`, `agent.rs` (subagent-spawn tool,
`SUBAGENT_TOOL_NAME`), `plan.rs` / `exit_plan_mode.rs`, `ask_question.rs`, `skill.rs`,
`registry.rs` (`base_tools`, `base_tools_read_only`).

### workspace (`agentloop-workspace`) — `crates/workspace/src/lib.rs`
Sole `git` edge: `Workspaces` impl — worktree isolation, per-turn snapshots
(`stash create` → shadow ref) for `/undo`/`/redo`.

### mcp (`agentloop-mcp`) — `crates/mcp/src/`
MCP client → `Tool` bridge via `rmcp`: `client.rs`, `manager.rs` (`McpManager`),
`bridge.rs`, `config.rs`, `tool.rs`.

### transports/stdio (`agentloop-transport-stdio`) — `crates/transports/stdio/src/lib.rs`
NDJSON wire framing over `EngineService`: `OneTurnRequest`, `event_visible()`.

### testkit (`agentloop-testkit`, dev-dep only) — `crates/testkit/src/`
`mock_provider.rs` (`MockProvider`), `mock_workspace.rs`, `store_conformance.rs`,
`scenario.rs` (scripted stdio), `fixtures.rs`, `tools.rs`.

### xtask — `xtask/src/main.rs`
`cargo xtask schema [--check]`: regenerates/validates `schemas/v1/*.schema.json` from
`contracts`' schemars types; CI fails on drift.

## packages/providers (connector umbrella)

### providers (facade, `agentloop-providers`) — `crates/providers/src/`
`resolve.rs`: `resolve_real_providers`/`resolve_available_providers`,
`CustomProviderSpec`, `BUILTIN_PROVIDER_IDS`, `COMPAT_PRESETS` (deepseek, openrouter,
groq, mistral, xai), `connect_bedrock`, `native`/`native_all`. Re-exports every
connector.

### LLM clients — `crates/{anthropic,openai,gemini,ollama,bedrock,copilot,chatgpt}/src/`
Each has `provider.rs` (stream → `ProviderStreamEvent` mapping), `wire.rs`, `config.rs`.
Extras: `openai/src/compat.rs` (OpenAI-compatible endpoints) and `oauth.rs`;
`bedrock/src/{eventstream,sigv4}.rs` (AWS framing + signing);
`copilot/src/device_flow.rs` (GitHub device-flow sign-in, `store_github_token`);
`chatgpt/` — ChatGPT Plus/Pro subscription via Codex Responses
(`chatgpt.com/backend-api/codex/responses`), reusing `openai` OAuth.

### common (`agentloop-provider-common`) — `crates/common/src/`
Shared client plumbing: `http.rs`, `sse.rs`, `env.rs`.

### delegators — `crates/delegators/{common,acp,claude-code,copilot,cursor,grok,opencode}/src/`
External-agent `Agent` impls (connectors). `common`: `line_agent.rs`, `stream_host.rs`,
`tokio_host.rs` (subprocess scaffolding). Each connector: `mapper.rs` (`EventMapper` →
`AgentEvent`; unit tests embed raw captured CLI stdout fixtures — see
`.claude/skills/record-fixtures`), `profile.rs`, `config.rs`; `acp` adds `client.rs` +
`protocol.rs`.

## packages/search

### search (`agentloop-search`) — `crates/search/src/`
`SearchPlugin` (`core::Plugin`): `plugin.rs` (optional
`with_researcher_models` for cheap-model pinning), `search_web.rs`,
`search_backend/` (`SearchBackend` trait + `chain`, `brave`, `ddg_instant`,
`wikipedia`, `ddg_html`, `searxng` — default chain: Instant Answer + Wikipedia;
optional Brave / `SEARXNG_BASE_URL`), `scrape_page/` (tool + `extract`
heuristics; reqwest + htmd), `rerank.rs`. Contributes the `researcher` role.

## packages/index

### index (`agentloop-index`) — `crates/index/src/`
`IndexPlugin` (`core::Plugin`): `scanner` + `chunker` + BM25 `lexical` + `symbols` +
optional `embed`/`vector_store` (RRF hybrid in `retrieve`), `repomap` (PageRank over
imports), tools `SearchCode` / `FindSymbol` / `RepoMap`, optional `AutoContextHook`
(`AGENTLOOP_AUTO_CONTEXT` / prefs `autoContext`, default off). Index refresh on tool
use is opt-in (`AGENTLOOP_INDEX_AUTO_UPDATE` / prefs `autoUpdateIndex`, default off —
reuse warm on-disk index; Settings Rebuild forces a refresh). On-disk index under
app-data `agentloop/index/<repo-hash>` (never inside the repo). Status via
`status_for` / `rebuild_with_stats` (desktop polls; no AgentEvent). Offline
retrieval eval gate in `eval` (golden query→file set, MockEmbedder, recall@10 ≥ 0.8;
CI: `cargo test -p agentloop-index --test retrieval_eval_gate`).

## packages/sdk

### sdk (`agentloop-sdk`) — `crates/sdk/src/`
Composition root: `lib.rs` (`AgentBuilder`: `.provider()`, `.model()`, `.cwd()`,
`.enable_plugin()`, `.executor()`, `.injection_scan()` …), `role_tiers.rs`
(`apply_research_model_tiers`: searcher/researcher → cheap, worker → strong when
a known provider is registered), `resolve.rs`,
`loop_agent.rs` (`LoopAgent` trait + `ClawBot` turn-by-turn driver), `run.rs`, `cli.rs`,
`eval_cmd.rs`, `main.rs` (the runner `[[bin]]`: NDJSON/stdio + doctor). Nothing depends
on this crate.

### eval (`agentloop-eval`) — `crates/eval/src/`
TOML task benchmark harness: `task.rs` (`CheckSpec`), `runner.rs`, `metrics.rs`,
`report.rs`. Task definitions + fixtures in `packages/sdk/evals/tasks/`.
(Retrieval recall@10 CI gate lives in `packages/index` `eval`, not here — offline
index scoring needs no `EngineService`.)

## packages/desktop

Tauri 2 + React desktop shell. Atomic Design UI catalog:
[`packages/desktop/COMPONENTS.md`](packages/desktop/COMPONENTS.md); layout /
spacing / positioning: [`packages/desktop/DESIGN.md`](packages/desktop/DESIGN.md).
Rust side
(`src-tauri`) is a thin command layer over `agentloop-sdk::AgentBuilder` (native
providers only; no delegators). Sessions persist via `JsonlStore` at
`dirs::data_dir()/agentloop/desktop/sessions/` (engine-owned store, desktop-scoped
path); secrets via OS keychain / encrypted file. GitHub Copilot uses device-flow
commands (`copilot_auth_*`) that call
`providers::copilot::{DeviceFlow, store_github_token}`. ChatGPT subscription uses
`chatgpt_auth_*` (headless OAuth via `providers::openai::{start_oauth, store_oauth_tokens}`)
to unlock the native `chatgpt` provider.

**Remote Access** (`src-tauri/src/remote/`): optional in-process HTTP/SSE control
plane for mobile clients. Settings → Remote Access enables a shared listener and
pluggable connection methods (manual, LAN, Bonjour/mDNS, public port; Cloudflare
Tunnel and Bluetooth are adapter stubs). Clients pair with a versioned JSON
document (host/port/token or Bonjour) and call `/v1/*` (sessions, prompt, SSE
events including deltas, permissions, questions, MCP, providers). This is
desktop-owned — not `agentloop-transport-http` / `flex serve`.

## packages/gateway

### channel (`agentloop-channel`) — `crates/channel/src/lib.rs`
Contract-only: `Channel` trait, `ChatKey`, `Inbound::{Message,PermissionReply}`,
`Outbound::{Text,PermissionRequest,Status}`, `ChannelError`. Adapters (Telegram, Slack,
Discord) and the session router are not implemented yet.

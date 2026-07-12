# ARCHITECTURE.md — system map

Derived documentation for humans and coding agents: how the system is shaped and how
data flows through it, with file paths so you can jump straight to code instead of
grepping. **AGENTS.md stays authoritative** for rules, contracts, and decisions — on
any conflict, AGENTS.md wins. Companion docs: [COMPONENTS.md](COMPONENTS.md) (per-crate
catalog) and [TECHSTACK.md](TECHSTACK.md) (dependencies and tooling).

## Big picture

An agent-loop engine in Rust: five sibling cargo workspaces under `packages/`, each with
its own `Cargo.lock`; cross-package dependencies are path deps back into
`packages/engine/crates/*`.

```
packages/engine     # provider-agnostic engine — the hub workspace
packages/providers  # LLM provider clients + external-agent connectors (delegators)
packages/search     # optional deep-search plugin (core::Plugin)
packages/sdk        # AgentBuilder composition root + the headless runner bin
packages/gateway    # chat-channel contract (Channel trait) — adapters not built yet
packages/desktop    # Tauri 2 + React UI — second composition root via AgentBuilder
```

One `Agent` trait (`packages/engine/crates/core/src/agent.rs`) with two implementation
families:

- **Native loop** — `NativeAgent` (`packages/engine/crates/loop/src/agent.rs`) calls LLM
  provider APIs directly through the `Provider` trait.
- **Connectors** — the delegator crates (`packages/providers/crates/delegators/*`) drive
  external coding agents (acp, claude-code, copilot, cursor, opencode) as subprocesses
  behind the same `Agent` interface.

All output is normalized into one canonical event stream (`AgentEvent`). The engine has
no UI of its own; clients attach via transports (NDJSON stdio, HTTP/SSE) or embed
`AgentBuilder` in-process (the `flex` CLI and `packages/desktop` Tauri shell).

Dependency shape is hub-and-spoke: `contracts` (pure data, no I/O) ← `core` (traits) ←
every other engine crate; engine crates never depend on siblings. The `engine` crate is
provider-agnostic — provider selection lives in the `providers` facade
(`packages/providers/crates/providers/src/resolve.rs`). Composition roots are
`packages/sdk` (`AgentBuilder` + `flex` bin) and `packages/desktop` (Tauri commands that
call the same builder); nothing depends on those roots.

## Turn lifecycle (event/data flow)

1. **Transport** — `packages/engine/crates/transports/stdio/src/lib.rs`: `OneTurnRequest`
   arrives as NDJSON; events go back out one `SessionEvent` per line, filtered by
   `event_visible()` / `OutputVerbosity`.
2. **Engine front door** — `packages/engine/crates/engine/src/lib.rs`:
   `EngineService::native(providers, default_model, config)` composes the
   `ProviderRegistry`, plugins, base tools, roles, commands, and prompt assembly, and
   wraps session operations (`create_session`, `prompt`, `respond_permission`,
   `integrate_session`/`discard_session`, `revert`).
3. **Loop** — `packages/engine/crates/loop/src/agent.rs`: `NativeAgent::prompt` resolves
   the `SessionHandle` (`session_handle.rs`) and runs the turn (`turn/mod.rs`).
   `turn/iteration.rs` builds a `ChatRequest` from
   `messages::transcript_to_messages(reduce(events))`, streams the model, and handles
   retries and mid-turn model failover (`TurnOptions.fallback_models`).
4. **Provider normalization boundary** — providers parse their own wire format and yield
   `core::ProviderStreamEvent` (`packages/engine/crates/core/src/provider.rs`); e.g.
   `packages/providers/crates/anthropic/src/provider.rs::stream_chat`. Raw provider
   quirks die here — nothing downstream of `contracts` sees them. Connectors do the same
   through per-delegator `EventMapper`s (`delegators/*/src/mapper.rs`).
5. **Tool dispatch** — `packages/engine/crates/loop/src/turn/tool_exec.rs::execute_one_call`
   runs each `ToolCall` on the bounded worker pool (`pool.rs`) after a permission
   `Verdict` (`permission.rs`, `rules.rs`). The reserved `Agent` tool bypasses the pool
   (`run_subagent_call` → `subagent.rs`): children are plain sessions of the same agent,
   relayed up as ephemeral `SubagentEvent`s.
6. **Persistence: append-then-broadcast** — every persisted `AgentEvent` goes through the
   `SessionHandle`, which appends to the `SessionStore`
   (`packages/engine/crates/session/src/{memory,jsonl}.rs`) *before* broadcasting to live
   subscribers. Events are ground truth for observability; tracing/metrics are derived at
   this choke point.
7. **Transcript** — `packages/engine/crates/contracts/src/reduce.rs::reduce(events) ->
   Transcript` is a pure projection, never stored. It rebuilds model context each turn
   and serves resumed sessions.

Layer contract (see AGENTS.md for the canonical table): streaming deltas
(`MarkdownDelta`, `ThinkingDelta`, `ToolArgsDelta`) are never persisted; materialized
items (`UserMessage`, `AssistantMessage`, `ToolCallUpdated`, `TurnCompleted`, …) always
are; the transcript is always derived.

## Cross-cutting seams

- **Plugins** — `core::Plugin` (`packages/engine/crates/core/src/plugin.rs`): contributes
  `tools()`, `system_prompt_fragment()`, `roles()`, `hooks()`; folded in by
  `EngineService::native`, enabled at the composition root
  (`AgentBuilder::enable_plugin`). First plugins: `search`
  (`packages/search/crates/search/src/plugin.rs`) and `learning`
  (`packages/engine/crates/learning/src/lib.rs`).
- **Hooks** — `core::Hook` (`packages/engine/crates/core/src/hook.rs`) with built-ins in
  `packages/engine/crates/hooks/src/`: `DiagnosticsHook`, `FormatOnEditHook`,
  `InjectionScanHook` (prompt-injection scanning of tool results, off by default).
- **Workspace isolation & snapshots** — `core::Workspaces`
  (`packages/engine/crates/core/src/workspace.rs`); sole implementation shells out to git
  (`packages/engine/crates/workspace/src/lib.rs`). Root sessions can get a git worktree
  (verify-then-merge lifecycle); per-turn snapshots back `/undo`/`/redo`. Spawnable roles
  with `isolation != Never` get per-subagent worktrees.
- **Command execution** — `core::Executor` (`packages/engine/crates/core/src/executor.rs`)
  with backends in `packages/engine/crates/executors/src/` (local `/bin/sh`, docker, ssh,
  apptainer, serverless stub). `BashTool` takes an `Arc<dyn Executor>`; only
  Shell-category tools execute remotely.
- **Roles & subagents** — `RoleRegistry`/`RoleSpec`
  (`packages/engine/crates/loop/src/roles.rs`): per-role tool filters, model chains
  (task-based routing), isolation policy, depth limits. Spawned via the `Agent` tool
  (`packages/engine/crates/tools/src/agent.rs`).
- **Permissions** — `packages/engine/crates/loop/src/{permission,rules}.rs`: every tool
  call is a tracked `ToolCall` with a persisted verdict; child permission asks route back
  through the agent-global pending map.
- **Prompt assembly** — `packages/engine/crates/prompts/src/assembler.rs` composes the
  system prompt from parts in `packages/engine/prompts/system/*.md`, plus plugin
  fragments, memory (`load_memory_section`), skills, and slash commands
  (`commands.rs`).
- **MCP** — `packages/engine/crates/mcp/src/` bridges MCP servers (via `rmcp`) into the
  `Tool` registry (`McpManager`).
- **Gateway contract** — `packages/gateway/crates/channel/src/lib.rs`: `Channel` trait +
  normalized `ChatKey`/`Inbound`/`Outbound`. Adapters and the session router are future
  work.

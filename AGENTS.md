# AGENTS.md — the one guide

This is the single source of truth for humans and coding agents working on this repo.
There is deliberately no CLAUDE.md, no CONTRIBUTING.md, no docs/ folder, no ADR files:
everything lives here or in code. Keep it that way.

## What this project is

An agent-loop engine in Rust, split into four sibling `packages/*` cargo workspaces:

- **`packages/engine`** — the provider-agnostic native loop and `EngineService`. Knows nothing
  about concrete providers; it composes over a prebuilt `ProviderRegistry` and a config-gated
  `Plugin` system. No `flex` bin lives here.
- **`packages/providers`** — the connector umbrella: LLM `Provider` clients (anthropic, openai,
  gemini, ollama, bedrock, copilot) *and* the external-agent connectors (formerly "delegators":
  acp, claude-code, copilot, cursor, opencode — still `Agent` impls), plus a `providers` facade
  crate that holds the provider-resolution logic and hands a registry to the engine.
- **`packages/search`** — an optional deep-search plugin (`core::Plugin`): `search_web` +
  `scrape_page` tools and a `researcher` role, enabled/disabled by config.
- **`packages/sdk`** — a builder API composing providers + engine + plugins, and the `flex`
  headless runner bin.

One `Agent` interface with two families of implementations: a **native loop** that calls LLM
provider APIs directly, and **connectors** that drive external coding agents behind the same
interface. All output is normalized into one canonical event stream. The engine itself has no
UI — transports (NDJSON stdio, ACP) let clients attach.

## Flex as a harness

An *agent harness* is the runtime scaffolding around a model that turns a raw
completion API into an agent that safely does work. Flex owns every part of
that anatomy, and it helps to name where each lives:

- **Loop / control** — `loop` (`NativeAgent`): turn iteration, tool dispatch on
  a bounded worker pool, cancellation, mid-turn model failover.
- **Tool interface & dispatch** — `core::Tool` + `ToolContext`; every path-taking
  tool sandboxes to `ToolContext.cwd` (populated from `SessionMeta.cwd`).
- **Context & memory** — `session` (append-only JSONL log = ground truth) +
  loop compaction; the transcript is a pure `reduce(events)` projection.
- **Model adapter** — `core::Provider` + `providers/*`; provider quirks die at
  the normalization boundary.
- **Guardrails / permissions** — `loop::permission` + rules; every tool call is
  a tracked `ToolCall` with a persisted verdict.
- **Observability** — persisted `AgentEvent`s (append-then-broadcast) with
  tracing/metrics derived at the choke point.
- **Environment / isolation** — `core::Workspaces` + `workspace` (git worktrees):
  a root session can run in an isolated working copy, reviewed and merged back.
  The same trait also snapshots the working tree per turn (git `stash create` →
  a shadow ref, no HEAD/branch pollution) so `/undo` and `/redo` can rewind file
  changes; availability-gated on git, works with or without isolation.
- **Composition / subagents** — the `Task` tool spawns role-scoped child
  sessions; children inherit the parent's cwd (and thus its isolated workspace).

Isolation is opt-in and top-level only: a root session whose effective
`IsolationPolicy` (from `NewSessionParams.isolation`, else its role's policy,
else the engine default) asks for it is provisioned a git worktree at
`create_session` (depth 0); `EngineService::{integrate,discard}_session` verify
and merge it back or drop it. Subagents never provision their own — they share
the parent's tree. The loop stays free of process code: it calls the injected
`Workspaces` trait, whose only implementation (`workspace` crate) shells out to
`git`. `Workspaces::{snapshot,restore}` back per-turn checkpoints: the loop
snapshots at each completed turn and emits an additive `SnapshotCreated` event;
`EngineService::revert` restores a snapshot (via `read-tree -u --reset`, never
moving a branch) and records a `SnapshotRestored` marker. The CLI's `/undo` and
`/redo` walk that snapshot timeline. Snapshots cover tracked files only, so a
restore never deletes brand-new untracked files.

## Repo map

Each package is its own cargo workspace with its own `Cargo.lock`; cross-package deps are path
deps (`providers`/`search`/`sdk` path-depend on `packages/engine/crates/*`).

```
packages/engine/              # provider-agnostic engine — the hub workspace
  prompts/                    # DATA: system-prompt parts + built-in slash-command templates
  schemas/v1/                 # generated JSON Schemas (cargo xtask schema); CI fails on drift
  crates/
    contracts/                # pure data: events, content blocks, ToolCall, ids, caps, errors,
                              # reducer, markdown projection, branding. serde+schemars+uuid only.
    core/                     # traits (Agent, Provider, Tool, SessionStore, Hook, Workspaces,
                              # Plugin) + registries (Tool/Provider/Plugin)
    loop/                     # NativeAgent — the agent loop (roles, subagents, model failover,
                              # bounded tool worker pool, workspace provisioning at depth 0)
    engine/                   # EngineService front door (provider-agnostic: takes a prebuilt
                              # ProviderRegistry + default ModelRef + EngineConfig, folds in plugins)
    prompts/                  # system-prompt assembly + slash-command registry/expansion
    session/                  # SessionStore impls (memory, jsonl)
    tools/                    # base tool set (Read/Write/Edit/Glob/Grep/Bash/WebFetch/...)
    workspace/                # Workspaces impl: git-worktree session isolation (the sole git edge)
    mcp/                      # MCP client (rmcp) -> Tool bridge
    transports/{stdio}        # serve any Agent to external clients
    testkit/                  # MockProvider, conformance suites, scripted-stdio (dev-dep only)
packages/providers/           # connector umbrella — its own workspace
  crates/
    providers/                # facade: resolve_{real,available}_providers, CustomProviderSpec,
                              # connect_bedrock, native/native_all; re-exports every connector
    {anthropic,openai,gemini,ollama,bedrock,copilot}/   # LLM Provider clients
    common/                   # shared provider client utilities
    delegators/{common,acp,claude-code,copilot,cursor,opencode}/   # external-agent Agent impls
packages/search/              # deep-search plugin — its own workspace
  crates/search/              # SearchPlugin (core::Plugin): search_web + scrape_page + researcher role
packages/sdk/                 # embeddable SDK + runner — its own workspace
  crates/
    sdk/                      # AgentBuilder + flex runner bin (NDJSON/stdio + doctor)
```

## Layer contract (do not collapse these)

| Layer | Type | Persisted? |
|---|---|---|
| Streaming deltas | `AgentEvent::{MarkdownDelta, ThinkingDelta, ToolArgsDelta, ...}` | never |
| Materialized items | `AgentEvent::{UserMessage, AssistantMessage, ToolCallUpdated, ...}` | always |
| Transcript | `reduce(events) -> Transcript` (pure) | derived, never stored |

Raw provider/agent wire formats die at the normalization layer (provider stream mapping,
delegator `EventMapper`). Nothing downstream of `contracts` may see a provider quirk.

## Dependency rules (hub-and-spoke)

- `contracts` depends on nothing heavy: serde, serde_json, schemars, uuid. No tokio, no I/O.
- `core` depends only on `contracts` (+ tokio-util/futures/async-trait).
- Every other engine crate depends on `core`/`contracts` — never on a sibling.
- The `engine` crate is **provider-agnostic**: it depends on no `providers/*` or `delegators/*`
  crate. Provider selection/construction lives in `packages/providers` (the `providers` facade),
  which path-depends back on `engine`'s `contracts`/`core`/`engine`.
- `packages/providers/*`: LLM clients depend on `providers/common`; connectors depend on
  `delegators/common`. The `providers` facade depends on all of them plus `agentloop-engine`.
- `packages/sdk`'s `runner` is the sole composition root; nothing depends on `runner`.
- I/O only at the edges. `loop` contains no HTTP and no process code.

### Plugins

`core::Plugin` is the extension seam: a plugin contributes `tools()`, a
`system_prompt_fragment()`, and `roles()` (expressed with the loop-independent `core::PluginRole`,
mapped to the loop's `RoleSpec` at composition). `EngineConfig.plugins` carries the enabled
plugins; `EngineService::native` folds them into the tool registry, appends prompt fragments to
the assembled system prompt, and merges their roles into the role registry. Enable/disable happens
at the composition root (`AgentBuilder::enable_plugin("search")`).

## Hard rules

1. **Brand in one place.** The product name appears in code exactly once:
   `crates/contracts/src/branding.rs`. Never in crate names, module paths, type/trait/function
   names, event discriminators, wire fields, error messages, or test names. CI greps for leaks.
2. **Generic names.** Files, modules, traits, and types use domain terms only:
   `Engine`, `Agent`, `Tool`, `ToolCall`, `Provider`, `Session`, `Event`, `Transcript`, `Command`.
3. **Markdown policy.** The only .md files in this repo: `README.md`, this file, and
   `packages/engine/prompts/**/*.md` (data). Do not add others.
4. **No `unwrap`/`expect` in library crates.** thiserror enums per crate, `#[non_exhaustive]`
   on public enums. `anyhow` only in `runner` and tests.
5. **tracing, not println.** Libraries never install subscribers; `runner` does.
6. **Cancellation is not an error.** Every await point must be cancel-safe; tools receive a
   `CancellationToken` and must honor it.
7. **Every parser/reducer change needs a golden test.** Never hand-edit recorded fixtures or
   `.snap` files to make a test pass — re-record and explain in the commit message.
8. **Workspace deps only.** Member Cargo.tomls contain no version numbers; every dependency is
   `{ workspace = true }` against the root `[workspace.dependencies]` table.
9. **Wire types are additive.** Changing `contracts` wire types within a protocol version must be
   additive (new variants, new optional fields). Consumers route unknown variants to `Unknown`.
10. **Tool design is prompt design.** Tool descriptions are written for the model: examples,
    edge cases, poka-yoke parameters, and error messages that teach the correct next step.
    Outputs are token-efficient — truncate with explicit markers, never silently.

## Key decisions log

- **Dual MIT OR Apache-2.0** (Rust convention; cargo-deny enforces compatible transitive licenses).
- **Own thin provider clients, not genai/rig**: thinking-signature round-trip, cache breakpoints,
  fine-grained tool-arg streaming, and retry semantics are the product differentiators and are
  hidden by generic multi-provider crates.
- **Own canonical contract, not ACP**: ACP lacks vocabulary for agent-impl selection, subagent
  trees, seq replay, cost accounting. ACP is one transport/adapter dialect among several.
- **Events are ground truth for observability**: timings/usage/cost/statuses live in persisted
  events; tracing spans and metrics are derived views recorded at the append+broadcast choke point.
- **JSONL session store first** (append-only maps 1:1, human-greppable); SQLite behind a feature later.
- **`packages/` monorepo, four workspaces**: `engine`, `providers`, `search`, `sdk` are siblings
  with their own toolchains and `Cargo.lock`s. `providers`/`search`/`sdk` path-depend on
  `packages/engine/crates/*`; those crates keep belonging to the engine workspace (so their
  `{ workspace = true }` deps and lints resolve against `packages/engine`). Repo root stays
  language-neutral.
- **The engine is provider-agnostic**: `EngineService::native(providers, default_model, config)`
  takes an already-built `ProviderRegistry`. Provider selection/construction moved to the
  `packages/providers` facade (`resolve_{real,available}_providers`, `CustomProviderSpec`,
  `connect_bedrock`, `native`/`native_all`). Delegators are relocated there too and re-branded as
  connectors, but keep the `Agent` trait (the thin `Provider` trait can't model an external agent
  that runs its own loop). This keeps the engine free of every provider/delegator dependency.
- **Plugins are a config-gated `core` trait**: `core::Plugin` (`tools`/`system_prompt_fragment`/
  `roles`) + `PluginRegistry`, folded into composition by `EngineService::native`. Roles use the
  loop-independent `core::PluginRole` (mapped to `RoleSpec` in the engine, since `RoleSpec` lives
  in `loop`). The SDK's `AgentBuilder::enable_plugin` decides which ship.
- **`packages/search` is the first plugin**: `search_web` (DuckDuckGo HTML, no paid API, swappable
  `SearchBackend`) + `scrape_page` (reqwest + htmd) tools and a `researcher` role whose prompt
  encodes an Analyze/Plan → Execute/Evaluate → Synthesis/Citation workflow. Dispatchable via `Task`.
- **`packages/sdk` is the composition root**: `AgentBuilder` composes the providers facade + native
  `EngineService` + enabled plugins; the `flex` `[[bin]]` (runner, NDJSON/stdio + doctor) lives
  here. The brand-gate exemption moved from the engine's `crates/runner/Cargo.toml` to the SDK's.
- **Model failover is loop-owned**: `TurnOptions.fallback_models` + the persisted
  `ModelFallback` event; eligible provider failures advance the chain mid-turn with
  partial drafts discarded pre-materialization. Role chains (v3) feed the same field.
- **Tools run on a bounded worker pool; subagents are loop-intercepted**: tool jobs
  are spawned tasks behind a semaphore (real parallelism, panic isolation via
  `JoinError::is_panic`); `Task` calls bypass the pool entirely — control plane ≠
  worker plane. Children are plain sessions of the same agent (role-scoped tools,
  role model chains, split round-robin), relayed into the parent as ephemeral
  `SubagentEvent`s; clients route child permission asks back by session id against
  the agent-global pending map.
- **Copilot device-flow sign-in lives in `providers/copilot`** (`device_flow.rs` +
  `store_github_token`): provider crates are the sanctioned I/O edges, and the apps.json
  format knowledge stays next to `discover_github_token`. The stored token interoperates
  with VS Code/JetBrains sign-ins (merge-upsert, never clobber).
- **Workspace isolation is an injected trait, top-level only, opt-in**: `core::Workspaces`
  (impl `workspace` crate, the sole `git` edge) keeps process code out of `loop`, mirroring
  `SessionStore`/`Provider` injection. Only depth-0 sessions provision a worktree (subagents
  inherit the parent's cwd — no per-child worktrees, no N-way merge); redirecting the single
  `SessionMeta.cwd → ToolContext.cwd` value sandboxes every tool with no tool changes.
  Trigger is role-declared (`RoleSpec.isolation`) with a `NewSessionParams.isolation` /
  `--isolate` override; default `Never` keeps behavior byte-identical. Lifecycle is
  verify-then-merge (`integrate_session` commits, runs the configured verify command, and
  fast-forwards the base — else keeps the worktree); `SessionMeta.base_cwd` lets resume fall
  back when a workspace is gone. Wire additions (`IsolationPolicy`, the `Workspace*` events,
  `SessionMeta.{isolation,workspace_id,base_cwd}`) are additive.
- **Engine roadmap** (north star: true parallelism and fault isolation via actors,
  distribution by default, swarms/metaswarms of any models, no bloat):
  - **Session actor** — the tool worker pool ships; remaining: a single-writer
    SessionActor mailbox (fixes the latent append→broadcast ordering race),
    turn-panic supervision, and a testkit conformance suite.
  - **Distributed nodes** — node capability tags on `ToolContext`; policy enforced in the
    permission layer (secret/sandboxed execution) — e.g. only some cluster nodes may
    read secret code.
  - **Metaswarms** — subagents ship (roles, tree UI, permission relay, cap enabled);
    deeper trees = raise `max_depth` (children currently lose `Task` at depth 1).
    Per-subagent worktree isolation (each parallel worker in its own tree, merged
    independently) is a planned extension of today's top-level-only isolation.
  - **Token-efficient meta-tools** — extend the eight base tools with `BatchRead`, `RepoMap`,
    `SymbolSearch`, and structured summaries.
  - **Reasoning fidelity** — provider thinking signatures ship (v2); remaining: thinking
    duration on `TurnCompleted`.

## Verify (run before every commit)

Each package is its own workspace; run the full verify in every package the change touches.
`providers`, `search`, and `sdk` compile the engine crates as path deps, so a change to
`packages/engine` should be verified in the downstream packages too.

```bash
for pkg in engine providers search sdk; do
  ( cd packages/$pkg
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace --all-features )
done
# engine only: schema drift gate
( cd packages/engine && cargo xtask schema --check )
```

**Install the runner globally**: `./install.sh` (repo root) builds the release binary from
`packages/sdk` and copies `flex` to `~/.local/bin` (override with `FLEX_BIN_DIR`). The binary
defaults its workdir to the current directory, so `cd my-project && flex doctor` works from any
project. `flex run -p "..."` streams one turn as NDJSON.

**Providers facade**: `agentloop_providers::{native, native_all}` resolve a `ProviderRegistry`
(single preferred provider, or every provider whose credentials resolve) and hand it to
`EngineService::native`. `CustomProviderSpec` + `resolve_*` live here, as does the
`BUILTIN_PROVIDER_IDS` shadowing guard.

DeepSeek (built-in, in the facade): set `DEEPSEEK_API_KEY` (optional `DEEPSEEK_MODEL`, default
`deepseek-v4-pro`). It's an OpenAI-compatible provider (`api.deepseek.com/v1`) built on
`OpenAiProvider`. Model ids: **`deepseek-v4-pro`** (strong) and **`deepseek-v4-flash`**
(fast/cheap); the legacy `deepseek-chat`/`deepseek-reasoner` names are deprecated and route to
v4-flash — don't use them. `deepseek` and `openai` are deliberately **not** in
`BUILTIN_PROVIDER_IDS`, so a user's custom spec of either id resolves and wins over the env
built-in (the env registration is skipped when a custom spec claims the id, so it's never
registered twice). dSpark speculative decoding is applied server-side and is transparent to API
clients — no request-time knob.

Task-based model routing: the `Task` tool's role list (`RoleRegistry::spawnable`) advertises each
role's **model** (e.g. `searcher — …-flash, read-only tools` · `worker — …-pro, full tool
access`), so the planner routes research → fast model, implementation → strong model. `subagent.rs`
runs the child turn with `model: chain.first()`. Split round-robin (`assigned_model`) engages only
for a role with a **multi-model** chain (`spec.split && models.len() >= 2`).

Brand-leak gates (CI runs these per package; each must print nothing):

```bash
git grep -iIl 'flex' -- packages/engine/crates/ \
  ':!packages/engine/crates/contracts/src/branding.rs'
git grep -iIl 'flex' -- packages/providers/crates/
git grep -iIl 'flex' -- packages/search/crates/
git grep -iIl 'flex' -- packages/sdk/crates/ ':!packages/sdk/crates/sdk/Cargo.toml'
```

Snapshot tests use `insta`. To (re)generate intentionally:
`INSTA_UPDATE=always cargo test -p <crate>` — then review the `.snap` diff like code.

## Commit style

`<area>: present-tense summary` (e.g. `contracts: add ToolCall status machine`).
Never commit fixtures containing real API keys. Do not push unless asked.

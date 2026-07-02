# AGENTS.md — the one guide

This is the single source of truth for humans and coding agents working on this repo.
There is deliberately no CLAUDE.md, no CONTRIBUTING.md, no docs/ folder, no ADR files:
everything lives here or in code. Keep it that way.

## What this project is

An agent-loop engine in Rust. One `Agent` interface with two families of implementations:
a **native loop** that calls LLM provider APIs directly (Anthropic, OpenAI, Gemini, Ollama),
and **delegators** that drive external coding agents (Claude Code, ACP agents, opencode, Cursor)
behind the same interface. All output is normalized into one canonical event stream.
The engine itself has no UI — transports (NDJSON stdio, ACP) let clients attach.
`packages/cli` is the first such client: an interactive ratatui terminal UI.

## Repo map

```
packages/engine/              # self-contained Rust cargo workspace — run all cargo commands here
  prompts/                    # DATA: system-prompt parts + built-in slash-command templates
  schemas/v1/                 # generated JSON Schemas (cargo xtask schema); CI fails on drift
  crates/
    contracts/                # pure data: events, content blocks, ToolCall, ids, caps, errors,
                              # reducer, markdown projection, branding. serde+schemars+uuid only.
    core/                     # traits (Agent, Provider, Tool, SessionStore, Hook) + registries
    loop/                     # NativeAgent — the agent loop implementation
    engine/                   # EngineService front door, event bus, resolver
    prompts/                  # system-prompt assembly + slash-command registry/expansion
    session/                  # SessionStore impls (memory, jsonl)
    tools/                    # base tool set (Read/Write/Edit/Glob/Grep/Bash/WebFetch/...)
    mcp/                      # MCP client (rmcp) -> Tool bridge
    providers/{common,...}    # LLM provider clients (one crate per provider)
    delegators/{common,...}   # external-agent adapters (one crate per agent)
    transports/{stdio,acp}    # serve any Agent to external clients
    testkit/                  # MockProvider, conformance suites, scripted-stdio (dev-dep only)
    runner/                   # composition root; [[bin]] name = "agentic-studio"
packages/cli/                 # separate cargo workspace — interactive terminal client
  crates/
    core/                     # agentloop-cli-core: EngineHub (agent selection), session
                              # controller, model catalog, copilot login orchestration
    cli/                      # agentloop-cli: ratatui TUI; [[bin]] name = "agenticstudio"
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
- Every other crate depends on `core`/`contracts` — never on a sibling
  (exceptions: `providers/*` on `providers/common`, `delegators/*` on `delegators/common`).
- `runner` is the sole composition root; nothing depends on `runner`.
- I/O only at the edges. `loop` contains no HTTP and no process code.

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
- **`packages/` monorepo**: `packages/engine` is self-contained; future modules (ui, cli, sdk)
  are siblings with their own toolchains. Repo root stays language-neutral.
- **`packages/cli` is a second composition root**: it wires engine crates by path and
  deliberately duplicates the runner's ~100-line agent-resolution pattern (probe + trace).
  Extract a shared crate only when a third client appears. Engine hard rules (brand gate,
  runner-only stdout) stop at the engine workspace boundary; the CLI mirrors the lint set
  and keeps crate names brand-free anyway, with the brand only in its `[[bin]]` name.
- **Copilot device-flow sign-in lives in `providers/copilot`** (`device_flow.rs` +
  `store_github_token`): provider crates are the sanctioned I/O edges, and the apps.json
  format knowledge stays next to `discover_github_token`. The stored token interoperates
  with VS Code/JetBrains sign-ins (merge-upsert, never clobber).
- **CLI phase 2+ engine roadmap** (documented only; not started):
  - **Actor parallelism** — today a single `NativeAgent` tokio loop; phase 2 splits a
    session actor from a tool worker pool while keeping the canonical event stream.
  - **Distributed nodes** — node capability tags on `ToolContext`; policy enforced in the
    permission layer (secret/sandboxed execution).
  - **Swarms / subagents** — `Subagent*` events exist; native cap `subagents: false` today;
    enable the cap and add a tree UI in the CLI.
  - **Token-efficient meta-tools** — extend the eight base tools with `BatchRead`, `RepoMap`,
    `SymbolSearch`, and structured summaries.
  - **Reasoning fidelity** — `ThinkingDelta` in contracts today; phase 2 adds provider thinking
    signatures and thinking duration on `TurnCompleted`.

## Verify (run before every commit)

```bash
cd packages/engine
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

If the change touches `packages/cli`, also:

```bash
cd packages/cli
cargo fmt --manifest-path crates/core/Cargo.toml --check
cargo fmt --manifest-path crates/cli/Cargo.toml --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

**CLI smoke checklist** (manual, on a machine with API keys):

1. `ANTHROPIC_API_KEY` → launch TUI, one turn streams markdown
2. `/models` lists anthropic and copilot concurrently
3. Mid-session `/model copilot/…` ↔ `/model anthropic/…` both stream
4. Clean `XDG_CONFIG_HOME` tempdir → `/agent copilot` (or `/provider copilot`) triggers inline device-flow sign-in, then copilot streams
5. Existing VS Code sign-in is picked up automatically (token merge-upsert on new sign-in)
6. `/agent claude-code` probes and streams (if CLI installed)
7. Esc cancels a mid-stream turn
8. `/mcp-install @modelcontextprotocol/server-memory` → `/mcps` toggle on → `/mcp memory` reloads native → `/mcp explore memory` lists tools

Automated subset (env-gated, skipped in CI):

```bash
AGENTLOOP_SMOKE=1 ANTHROPIC_API_KEY=... cargo test -p agentloop-cli-core smoke -- --ignored --nocapture
```

Custom providers (manual): add a `providers` entry to `~/.config/agentloop/config.json` with
`base_url`, `api_key` (`{env:VAR}` supported), optional `thinking: true`, then `/provider <id>`
or restart. No `/connect` wizard yet.

Brand-leak gate (CI runs this; must print nothing):

```bash
git grep -iIl 'agenticstudio' -- packages/engine/crates/ \
  ':!packages/engine/crates/contracts/src/branding.rs' \
  ':!packages/engine/crates/runner/Cargo.toml'
```

Snapshot tests use `insta`. To (re)generate intentionally:
`INSTA_UPDATE=always cargo test -p <crate>` — then review the `.snap` diff like code.

## Commit style

`<area>: present-tense summary` (e.g. `contracts: add ToolCall status machine`).
Never commit fixtures containing real API keys. Do not push unless asked.

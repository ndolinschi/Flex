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
    loop/                     # NativeAgent — the agent loop (roles, subagents, model failover,
                              # bounded tool worker pool)
    engine/                   # EngineService front door, event bus, resolver
    prompts/                  # system-prompt assembly + slash-command registry/expansion
    session/                  # SessionStore impls (memory, jsonl)
    tools/                    # base tool set (Read/Write/Edit/Glob/Grep/Bash/WebFetch/...)
    mcp/                      # MCP client (rmcp) -> Tool bridge
    providers/{common,...}    # LLM provider clients (one crate per provider)
    delegators/{common,...}   # external-agent adapters (one crate per agent)
    transports/{stdio,acp}    # serve any Agent to external clients
    testkit/                  # MockProvider, conformance suites, scripted-stdio (dev-dep only)
    runner/                   # composition root; [[bin]] name = "flex"
packages/cli/                 # separate cargo workspace — interactive terminal client
  crates/
    core/                     # agentloop-cli-core: EngineHub (agent selection), session
                              # controller, model catalog, copilot login orchestration
    cli/                      # agentloop-cli: ratatui TUI (subagent tree, role-tagged
                              # permission prompts); [[bin]] name = "flex"
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
  - **Token-efficient meta-tools** — extend the eight base tools with `BatchRead`, `RepoMap`,
    `SymbolSearch`, and structured summaries.
  - **Reasoning fidelity** — provider thinking signatures ship (v2); remaining: thinking
    duration on `TurnCompleted`.

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

**Install the CLI globally**: `./install.sh` (repo root) builds the release binary
and copies `flex` to `~/.local/bin` (override with `FLEX_BIN_DIR`).
The binary defaults its workdir to the current directory, so `cd my-project && flex`
opens a session there — no `--workdir` needed.

**CLI smoke checklist** (manual, on a machine with API keys):

1. `ANTHROPIC_API_KEY` → launch TUI, one turn streams markdown
2. `/models` lists anthropic and copilot concurrently
3. Mid-session `/model copilot/…` ↔ `/model anthropic/…` both stream
4. Clean `XDG_CONFIG_HOME` tempdir → `/agent copilot` (or `/provider copilot`) triggers inline device-flow sign-in, then copilot streams
5. Existing VS Code sign-in is picked up automatically (token merge-upsert on new sign-in)
6. `/agent claude-code` probes and streams (if CLI installed)
7. Esc cancels a mid-stream turn
8. `/mcp-install @modelcontextprotocol/server-memory` → `/mcps` toggle on → `/mcp memory` reloads native → `/mcp explore memory` lists tools
9. Mouse capture is **on by default** so the wheel scrolls the transcript and you can't scroll the terminal's own buffer up "over" the CLI (poor TUI behaviour). The escape-code garbage this once caused was really a UTF-8 truncation panic in `tool_exec.rs` (a `&s[..2000]` slicing a multi-byte char in pasted non-ASCII input — now trims to a char boundary) plus a panic hook that tore the terminal down on a *caught* worker-thread panic (now restores only on a main-thread panic) — not mouse capture itself. Selection with capture on needs a terminal modifier (⌥ iTerm2 · Fn Terminal.app · Shift Linux); Ctrl+M flips to no-modifier drag-select; the choice is persisted (`prefs.mouse_capture`, applied in `apply_loaded_prefs`). Bracketed paste (`EnableBracketedPaste`) is on so a pasted block is one `Event::Paste`, not keystrokes (a pasted newline would otherwise submit the prompt). Terminal teardown is defended on every exit path: `Drop` + a panic hook + a `spawn_signal_restore` task that restores on SIGTERM/SIGHUP/SIGQUIT (kill / closed tab / Ctrl+\) and swallows the job-control STOP signals SIGTSTP/SIGTTIN/SIGTTOU (Ctrl+Z, and background tty access while an engine/MCP reload spawns a child that grabs the terminal — the case that bit right after a provider switch). Swallowing keeps the app alive instead of frozen-with-tracking-on; quit with Ctrl+C/Ctrl+D/`/quit`. Belt-and-suspenders for the reload case: MCP stdio children are spawned in their own process group (`cmd.process_group(0)` in `mcp/src/client.rs`) so they can't wrestle the controlling terminal, and the runtime calls `TerminalSession::reassert` (force raw mode + alt screen + mouse) after every `EngineReloaded` and on each tick for ~2s afterward. A child can reset the shared tty's termios *after* the reload lands (turning ECHO back on while `?1000h` stays set, so scroll wheel reports echo as text), and crucially crossterm's `enable_raw_mode()` **no-ops once its flag says raw is on** — so re-asserting must go through `force_raw_mode()`, a `disable_raw_mode()`→`enable_raw_mode()` toggle that actually re-applies raw. The ~2s guard window force-toggles every 100ms tick (beating a late clobber) and repaints periodically to wipe whatever echoed before it caught it. Mouse mode is button+wheel-only (`?1000h`+`?1006h`, never `?1003h` motion), so even an unclean exit can't spew motion reports.

Automated subset (env-gated, skipped in CI):

```bash
AGENTLOOP_SMOKE=1 ANTHROPIC_API_KEY=... cargo test -p agentloop-cli-core smoke -- --ignored --nocapture
```

Custom providers (manual): add a `providers` entry to `~/.config/agentloop/config.json` with
`base_url`, `api_key` (`{env:VAR}` supported), optional `thinking: true`, then `/provider <id>`
or restart. No `/connect` wizard yet.

DeepSeek (built-in): set `DEEPSEEK_API_KEY` (optional `DEEPSEEK_MODEL`, default
`deepseek-v4-pro`), or set the key from inside the CLI with the known-provider shortcut
`/connect deepseek <api_key>` (infers `base_url`/model — see `known_provider_defaults`). DeepSeek
then auto-registers like the other built-ins — `/provider deepseek` or `--provider deepseek`. It's
an OpenAI-compatible provider (`api.deepseek.com/v1`) built on `OpenAiProvider`; its 1M-token
context window is registered in `context_budget` so auto-compaction is accurate. Model ids:
**`deepseek-v4-pro`** (strong) and **`deepseek-v4-flash`** (fast/cheap); the legacy
`deepseek-chat`/`deepseek-reasoner` names are deprecated (2026-07-24) and just route to v4-flash —
don't use them. A user's own `/connect deepseek …` (a custom spec) still wins over the built-in —
so `deepseek` is deliberately **not** in `BUILTIN_PROVIDER_IDS` (it would otherwise reject that
custom spec as a conflict).

DeepSeek auto-orchestration: when the active provider/model is DeepSeek and the user hasn't
overridden the `searcher`/`worker` roles, `engines.rs::build_native` **automatically** applies the
split (in-memory, not persisted) — research subagents (`searcher`) on `deepseek-v4-flash`,
implementation (`worker`) on `deepseek-v4-pro` — mirroring Claude Code's Haiku-search /
Sonnet-work default (`deepseek_is_active` gates it; `prefs::deepseek_roles_preset()` is the pure
mapping). `/roles preset deepseek` is the explicit, persistent form (also sets the session model
to v4-pro and reloads). Nothing DeepSeek-specific leaks into the engine — it's the generic role
model-chain mechanism either way.

End-to-end, task-based model routing works like this: the `Task` tool's role list
(`RoleRegistry::spawnable`) advertises each role's **model** (e.g. `searcher —
deepseek-v4-flash, read-only tools` · `worker — deepseek-v4-pro, full tool access`), so the
planner sees that research and implementation run different models and routes by task; the tool
description already steers "a question answerable by reading code → searcher(s); implementation →
one worker per independent task". `subagent.rs` then runs the child turn with
`model: chain.first()` where the chain starts with the role's models — so a `searcher` child
genuinely runs v4-flash and a `worker` child v4-pro. Split round-robin (`assigned_model`) only
engages for a role with a **multi-model** chain (`spec.split && models.len() >= 2`); DeepSeek's
single-model role chains are unaffected.

The model picker (`/model`, `open_catalog_picker`) is **scoped to the active provider** — a
DeepSeek session lists only DeepSeek models, not Copilot/Anthropic (filtered by
`current_provider()`; falls back to the full catalog when nothing matches). Its first row is an
**`auto`** entry for providers that have a smart mode (`auto_mode_detail` gates it; only DeepSeek
today). Selecting `auto` (or `/model deepseek/auto`) is a UX shortcut that routes through
`apply_auto_model` → `run_roles_preset("deepseek")` — i.e. v4-pro planner+worker, v4-flash
research. `<provider>/auto` is a sentinel, not a real model id: `set_model` intercepts it before
any real-model handling.

DeepSeek's **dSpark speculative decoding** (announced 2026) needs no integration here: it runs
server-side inside DeepSeek's inference stack and is transparent to API clients — calling
DeepSeek V4 already gets the speedup, and there is no request-time knob to add. To self-host the
acceleration, run `vllm serve deepseek-ai/DeepSeek-V4-Pro-DSpark` and point a custom provider (or
`OPENAI_BASE_URL`) at it; dSpark applies inside vLLM. (`github.com/deep-spark` is Iluvatar CoreX
hardware tooling — unrelated to DeepSeek, nothing for a client to use.)

Brand-leak gate (CI runs this; must print nothing):

```bash
git grep -iIl 'flex' -- packages/engine/crates/ \
  ':!packages/engine/crates/contracts/src/branding.rs' \
  ':!packages/engine/crates/runner/Cargo.toml'
```

Snapshot tests use `insta`. To (re)generate intentionally:
`INSTA_UPDATE=always cargo test -p <crate>` — then review the `.snap` diff like code.

## Commit style

`<area>: present-tense summary` (e.g. `contracts: add ToolCall status machine`).
Never commit fixtures containing real API keys. Do not push unless asked.

# Flex

> **Pre-alpha.** APIs, wire formats, and the project name itself are still moving.

An open-source **agent-loop engine** in Rust, split into composable packages:
a provider-agnostic native engine, a providers umbrella (LLM clients +
external-agent connectors), an optional deep-search plugin, and an embeddable
SDK with a headless runner. The engine is not a UI or a CLI product itself —
it's what powers them.

## What it is

- **One `Agent` interface, many implementations.** The native loop calls LLM
  APIs directly (Anthropic, OpenAI, Gemini, Ollama, Bedrock, Copilot,
  DeepSeek). Delegator implementations drive external coding agents — Claude
  Code, ACP agents (Gemini CLI, Goose), opencode, Copilot — behind the
  exact same interface. Any of them can run as a subagent of any other.
- **One stream format.** Every provider and external agent's output is
  normalized into a single typed, markdown-flavored event stream (text,
  thinking, tool calls, tool results). Consumers render one format, always.
- **First-class ToolCalls.** Every tool invocation is a tracked record:
  request, response, status machine, timing, permission trail.
- **Headless by contract.** A versioned wire protocol (NDJSON over stdio, ACP
  for editors) lets UIs, CLIs, and CI drive the engine — the same way
  `claude -p` works.
- **Observable by default.** Structured logs and metrics for every agent step,
  derived from the same canonical event stream that is persisted as the
  session's append-only log.
- **Plugins.** The engine ships with base tools (Read, Write, Edit, Glob, Grep,
  Bash, WebFetch). An optional `search` plugin adds `search_web`, `scrape_page`,
  and a `researcher` role for deep-research workflows. Plugins contribute
  tools, system-prompt fragments, and roles at composition time.
- **Multi-backend search.** The search plugin chains DuckDuckGo and SearXNG
  backends with automatic fallback. `scrape_page` strips navigation, headers,
  and footers from fetched HTML and extracts the content core — with optional
  link extraction for recursive deep-research exploration.

## Layout

```
├── AGENTS.md                  # architecture map, layer contracts, contributor rules
├── README.md                  # this file
├── install.sh                 # build + install the `flex` runner globally
├── packages/
│   ├── desktop/               # Tauri 2 + React desktop app — second composition root
│   ├── engine/                # cargo workspace — provider-agnostic native engine
│   │   ├── crates/
│   │   │   ├── contracts/     # pure data: events, content blocks, ToolCall, branding
│   │   │   ├── core/          # traits: Agent, Provider, Tool, SessionStore, Plugin, Workspaces
│   │   │   ├── loop/          # NativeAgent — turn iteration, tool dispatch, model failover
│   │   │   ├── engine/        # EngineService front door — provider-agnostic composition
│   │   │   ├── prompts/       # system-prompt assembly + slash-command registry
│   │   │   ├── session/       # SessionStore impls (memory, JSONL)
│   │   │   ├── tools/         # base tool set: Read/Write/Edit/Glob/Grep/Bash/WebFetch
│   │   │   ├── hooks/         # pre/post-turn hooks, formatters, diagnostics
│   │   │   ├── workspace/     # Workspaces impl: git-worktree session isolation
│   │   │   ├── mcp/           # MCP client (rmcp) → Tool bridge
│   │   │   ├── transports/    # stdio/NDJSON transport, ACP adapter
│   │   │   └── testkit/       # MockProvider, conformance suites (dev-dep only)
│   │   ├── prompts/           # DATA: system-prompt parts + built-in templates
│   │   └── schemas/           # generated JSON Schemas (cargo xtask)
│   ├── providers/             # cargo workspace — connector umbrella
│   │   └── crates/
│   │       ├── providers/     # facade: resolve provider registry, re-exports all connectors
│   │       ├── common/        # shared provider client utilities
│   │       ├── anthropic/     # Anthropic Messages API client
│   │       ├── openai/        # OpenAI (and DeepSeek) client
│   │       ├── gemini/        # Google Gemini client
│   │       ├── ollama/        # Ollama local model client
│   │       ├── bedrock/       # AWS Bedrock client
│   │       ├── copilot/       # GitHub Copilot device-flow auth + API client
│   │       └── delegators/    # external-agent connectors (acp, claude-code, copilot,
│   │                          #   opencode) + shared common
│   ├── search/                # cargo workspace — deep-search plugin
│   │   └── crates/search/     # SearchPlugin: search_web + scrape_page + researcher role
│   └── sdk/                   # cargo workspace — builder API + headless runner
│       └── crates/sdk/        # AgentBuilder + `flex` [[bin]]
```

## Quick start — the `flex` runner

Install globally and use from any project directory:

```bash
./install.sh                              # builds release binary → ~/.local/bin/flex
flex --version                            # prints version
flex doctor                               # shows resolved provider, model, workdir
```

Run one turn — the workdir defaults to the current directory:

```bash
cd my-project && flex run -p "summarize the README"
```

Or run **headless** (no local project, read-only tools):

```bash
flex run -p "what happened in fusion energy in 2024-2025?"
```

Pick a specific provider and model:

```bash
flex run --provider deepseek --model deepseek-v4-pro -p "explain this codebase"
flex run --provider anthropic --model claude-sonnet-4 -p "..." --workdir ~/my-project
```

Set an API key inline (bypasses environment variables):

```bash
flex run --provider deepseek -p "..." --key sk-...
```

## Desktop app

`packages/desktop` is a native desktop app (Tauri 2 + React) — a second
composition root over `AgentBuilder`, alongside the `flex` CLI. It's an
agents-first chat UI, not an IDE:

- Multi-provider sessions with named connection profiles, including AWS
  Bedrock bearer auth.
- Per-session git-worktree isolation with a review flow — per-file/hunk
  keep/undo, checkpoints.
- Embedded browser, a real PTY terminal, and a read-only terminal for
  watching the agent work.
- Plan mode with approval, a live subagent viewer, per-project and global
  memory, MCP servers, an effort picker, and session-scoped change tracking.

```bash
cd packages/desktop
npm install
npm run tauri dev
```

See [packages/desktop/README.md](packages/desktop/README.md) for prerequisites,
provider setup, and keyboard shortcuts.

## SDK — embed in your own Rust project

```toml
# Cargo.toml
[dependencies]
agentloop-sdk = { git = "https://github.com/ndolinschi/Flex", features = ["search"] }
```

```rust
use agentloop_sdk::{AgentBuilder, OutputVerbosity};

// Minimal: auto-detect provider from environment.
let service = AgentBuilder::new().build()?;

// Full: pick provider, model, plugins, verbosity, and headless mode.
let service = AgentBuilder::new()
    .provider("deepseek")
    .model("deepseek-v4-pro")
    .provider_key("deepseek", "sk-...")
    .enable_plugin("search")
    .verbosity(OutputVerbosity::Low)
    .headless()
    .build()?;
```

See [AGENTS.md](AGENTS.md) for the architecture map, layer contracts, and
contribution rules.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

# Flex

> **Pre-alpha.** APIs, wire formats, and the project name itself are still moving.

An open-source **agent-loop engine** in Rust, split into composable packages: a
provider-agnostic native engine, a providers umbrella (LLM clients + external-agent
connectors), an optional deep-search plugin, and an embeddable SDK that composes them.
The engine is not a UI or a CLI product itself — it's what powers them.

## What it is

- **One `Agent` interface, many implementations.** The native loop calls LLM APIs directly (Anthropic, OpenAI, Gemini, Ollama). *Delegator* implementations drive external coding agents — Claude Code, ACP agents (Gemini CLI, Goose), opencode, Cursor, Copilot — behind the exact same interface. Any of them can run as a subagent of any other.
- **One stream format.** Every provider's and every external agent's output is normalized into a single typed, markdown-flavored event stream (text, thinking, tool calls, tool results). Consumers render one format, always — and any session can be projected to a single readable markdown document.
- **First-class ToolCalls.** Every tool invocation is a tracked record: request, response, status machine, timing, permission trail. The session log answers "what ran, with what arguments, for how long, allowed by whom" — identically for native and delegated runs.
- **Headless by contract.** A versioned wire protocol (NDJSON over stdio, ACP for editors) lets UIs, CLIs, and CI drive the engine — the same way `claude -p` works.
- **Observable by default.** Structured logs and metrics for every agent step, derived from the same canonical event stream that is persisted as the session's append-only log.

## Layout

```
packages/engine/     # cargo workspace — provider-agnostic native engine
                     # crates: contracts, core, loop, engine, prompts, session,
                     # tools, mcp, hooks, workspace, transports/*
packages/providers/  # cargo workspace — connector umbrella
                     # crates: providers (facade), {anthropic,openai,gemini,ollama,
                     # bedrock,copilot} (Provider), delegators/* (Agent)
packages/search/     # cargo workspace — deep-search plugin (search_web + scrape_page)
packages/sdk/        # cargo workspace — builder API + the `flex` runner bin
```

Each package is its own cargo workspace with its own `Cargo.lock`; cross-package
dependencies are path deps. Build and test any package:

```bash
cd packages/engine    # or providers / search / sdk
cargo test --workspace --all-features
```

The `flex` headless runner lives in the SDK:

```bash
cd packages/sdk
cargo run -p agentloop-runner -- --version
```

Install the runner globally and use it from any project directory:

```bash
./install.sh                              # builds a release binary, installs to ~/.local/bin
cd my-project && flex doctor              # workdir defaults to the current directory
```

See [AGENTS.md](AGENTS.md) for the architecture map, layer contracts, and contribution rules.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

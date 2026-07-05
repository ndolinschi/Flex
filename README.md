# Flex

> **Pre-alpha.** APIs, wire formats, and the project name itself are still moving.

An open-source **agent-loop engine** in Rust, plus an interactive terminal client built on it.
The engine is not a UI or a CLI product itself — it's what powers them.

## What it is

- **One `Agent` interface, many implementations.** The native loop calls LLM APIs directly (Anthropic, OpenAI, Gemini, Ollama). *Delegator* implementations drive external coding agents — Claude Code, ACP agents (Gemini CLI, Goose), opencode, Cursor, Copilot — behind the exact same interface. Any of them can run as a subagent of any other.
- **One stream format.** Every provider's and every external agent's output is normalized into a single typed, markdown-flavored event stream (text, thinking, tool calls, tool results). Consumers render one format, always — and any session can be projected to a single readable markdown document.
- **First-class ToolCalls.** Every tool invocation is a tracked record: request, response, status machine, timing, permission trail. The session log answers "what ran, with what arguments, for how long, allowed by whom" — identically for native and delegated runs.
- **Headless by contract.** A versioned wire protocol (NDJSON over stdio, ACP for editors) lets UIs, CLIs, and CI drive the engine — the same way `claude -p` works.
- **Observable by default.** Structured logs and metrics for every agent step, derived from the same canonical event stream that is persisted as the session's append-only log.

## Layout

```
packages/engine/   # self-contained Rust cargo workspace — the engine itself
                   # crates: contracts, core, loop, engine, prompts, session, tools,
                   # mcp, providers/*, delegators/*, transports/*, runner
packages/cli/      # separate cargo workspace — interactive terminal client (ratatui)
                   # crates: core (agent selection, session control), cli (TUI, bin: flex)
```

Build and test the engine:

```bash
cd packages/engine
cargo test --workspace --all-features
cargo run -p agentloop-runner -- --version
```

Build and test the CLI:

```bash
cd packages/cli
cargo test --workspace --all-features
cargo run -p agentloop-cli
```

Or install the CLI globally and run it from any project directory:

```bash
./install.sh                              # builds a release binary, installs to ~/.local/bin
cd my-project && flex                     # workdir defaults to the current directory
```

See [AGENTS.md](AGENTS.md) for the architecture map, layer contracts, and contribution rules.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

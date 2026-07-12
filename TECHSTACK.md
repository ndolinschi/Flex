# TECHSTACK.md — languages, dependencies, tooling

Derived documentation — **AGENTS.md is authoritative** for rules and the verify loop.
Companions: [ARCHITECTURE.md](ARCHITECTURE.md), [COMPONENTS.md](COMPONENTS.md).

## Language & toolchain

- Rust, `edition = "2024"`, `rust-version = "1.85"`, `resolver = "3"`.
- Five independent cargo workspaces (`packages/{engine,providers,search,sdk,gateway}`),
  plus `packages/desktop` (Tauri 2 + Vite/React/TypeScript UI; not a cargo workspace
  member of the others — its `src-tauri` path-depends on `agentloop-sdk`),
  each with its own `Cargo.toml` root and `Cargo.lock`. Member crates use
  `{ workspace = true }` deps only — no version numbers in member Cargo.tomls.
- License: dual `MIT OR Apache-2.0`; transitive licenses enforced by cargo-deny in CI.
- Lint posture: `unsafe_code = "forbid"` workspace-wide; clippy `all = warn` plus
  `unwrap_used`, `expect_used`, `dbg_macro`, `todo`, `print_stdout`, `print_stderr`
  (promoted to errors by `-D warnings` in CI/verify). No `unwrap`/`expect` in library
  crates; `anyhow` only in the runner and tests; `tracing`, never `println`.

## Key dependencies

- **engine** (`packages/engine/Cargo.toml`): `serde`/`serde_json`, `schemars` 1 (JSON
  Schema derivation in `contracts`), `thiserror` 2, `uuid` (v7), `async-trait`,
  `futures`, `tokio` (rt-multi-thread, sync, time, io-util, fs, process, signal),
  `tokio-stream`/`tokio-util`, `tracing`(+subscriber), `metrics` 0.24, `reqwest`
  (rustls-tls, no default features), `globset`/`ignore`/`regex` (glob/grep tools),
  `htmd` (HTML→markdown), `rmcp` 2.1 (MCP client), `http` 1, `base64`, `sha2`.
  Dev: `insta` (json + redactions), `pretty_assertions`, `tempfile`, `temp_env`,
  `wiremock`.
- **providers**: same external set + path deps into `packages/engine/crates/*`.
- **search**: minimal — `reqwest`, `htmd`, a tokio subset, engine path deps.
- **sdk**: adds `toml` 0.8 (eval task parsing); path-depends on engine crates, the
  providers facade, and the search plugin (feature-gated: `search`, `learning`).
- **gateway**: leanest — `serde`, `tokio`, `reqwest`, `anyhow`; dev-only wiremock.

## Build, test & CI tooling

- **Verify loop** (before every commit; also `.claude/skills/verify`): in every touched
  package — `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings`, `cargo test --workspace --all-features`. Engine
  changes must also be verified in downstream packages (path deps).
- **Schema drift gate**: `cd packages/engine && cargo xtask schema --check` validates
  `schemas/v1/*.schema.json` against `contracts`' schemars types; `cargo xtask schema`
  regenerates. CI fails on drift.
- **Snapshot tests**: `insta`. Regenerate intentionally with
  `INSTA_UPDATE=always cargo test -p <crate>`, then review the `.snap` diff like code.
  Never hand-edit fixtures or `.snap` files; delegator wire-format fixtures follow
  `.claude/skills/record-fixtures`.
- **Testkit**: `packages/engine/crates/testkit` (dev-dep only) — `MockProvider`,
  mock workspace, store conformance suite, scripted-stdio scenarios.
- **Eval harness**: `packages/sdk/crates/eval` + the runner's `eval` subcommand; TOML
  task definitions and fixtures under `packages/sdk/evals/tasks/`.
- **CI** (`.github/workflows/ci.yml`): per-workspace lint/test jobs (ubuntu + macos
  matrix), `cargo doc` with `RUSTDOCFLAGS="-D warnings"`, cargo-deny
  (licenses/advisories), the schema drift gate, the **brand-leak gate** —
  `git grep -iIl` for the product name over `packages/*/crates/` must print nothing
  (sole exemptions: `contracts/src/branding.rs` and the SDK bin's `Cargo.toml`),
  desktop Vitest (`desktop-frontend`), and Playwright native-app-gate E2E
  (`desktop-e2e`). Nightly soak / env-gated provider matrix:
  `.github/workflows/nightly.yml`.
- **Desktop UI**: `packages/desktop` — Tauri 2, React 19, Tailwind v4, Zustand,
  TanStack Query, Playwright E2E for the Vite native-app-required gate; Rust
  shell composes via
  `agentloop-sdk::AgentBuilder` and stores API keys in the OS keychain
  (`keyring`). Run: `cd packages/desktop && pnpm tauri dev`. E2E:
  `pnpm test:e2e`.
- **Install**: `./scripts/install_mac.sh` (macOS) or `.\scripts\install_windows.ps1` /
  `./scripts/install_windows.sh` (Windows) builds the release runner from `packages/sdk` and
  the desktop app from `packages/desktop`, then installs CLI + app (`~/.local/bin` +
  `/Applications/Flex.app` on macOS; `%USERPROFILE%\.local\bin` +
  `%LOCALAPPDATA%\Programs\Flex` on Windows). Overrides: `FLEX_BIN_DIR`, `FLEX_APP_DIR`.
- **Harness config**: `.claude/settings.json` (permission allowlist for cargo/git
  commands) and `.claude/skills/` (verify, record-fixtures).

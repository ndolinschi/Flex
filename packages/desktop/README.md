# Desktop — Tauri UI for the agent-loop engine

In-process Tauri 2 + React desktop shell. Composes the engine via
`agentloop-sdk` (`AgentBuilder`) — the same composition root as the `flex` CLI.
Native providers only (no external-agent delegators), but with MCP servers,
engine plugins (Search / Learning / Verifier), and a multi-tier orchestration
mode wired in.

## Prerequisites

- Rust toolchain (see repo root)
- Node 20+
- Platform deps for [Tauri 2](https://v2.tauri.app/start/prerequisites/)

## Run

This package is **pnpm-only** — installing with npm produces a duplicate React
and breaks the app ("Invalid hook call").

```bash
cd packages/desktop
pnpm install
pnpm tauri dev
```

Browser UI preview (no Tauri, mock backend):

```bash
pnpm dev
```

### E2E (Playwright + browserMock) — PR gate

Fast UI smoke against Vite + `src/lib/browserMock.ts` (no Tauri, no API keys):

```bash
cd packages/desktop
pnpm install
pnpm exec playwright install chromium   # once per machine
pnpm test:e2e
```

Specs live in `e2e/`. Playwright starts Vite on `127.0.0.1:1420` automatically
unless `E2E_BASE_URL` is set (then it reuses that server). CI job: `desktop-e2e`
in `.github/workflows/ci.yml`.

Optional screenshot walk (manual): start `pnpm dev`, then
`node scripts/preview-verify.mjs`.

### Nightly soak / provider matrix

- **Soak (3.2 skeleton):** with `pnpm dev` running,
  `SOAK_TURNS=5 pnpm soak` — N mock turns + heap/DOM samples → `.soak/`.
  Nightly workflow: `.github/workflows/nightly.yml` → `soak-browser-mock`.
- **Provider matrix (3.4 skeleton):** env-gated, soft-pass without keys:
  `./packages/sdk/scripts/provider-matrix-smoke.sh`
  (nightly job `provider-matrix`). Missing `*_API_KEY` secrets skip providers.
- **osascript real-window smoke (3.1 slow):** stubbed/disabled in `nightly.yml`.
- **Follow-ups not automated yet:** large-repo battery (3.3), Win/Linux port
  pass (3.5).

Production builds:

```bash
pnpm build:mac   # macOS .app + .dmg
pnpm build:win   # Windows NSIS installer
```

From the repo root, platform install scripts also build and install the desktop
app next to the CLI:

```bash
./scripts/install_mac.sh                 # → /Applications/Flex.app
# Windows PowerShell:
#   .\scripts\install_windows.ps1        # → %LOCALAPPDATA%\Programs\Flex\Flex.exe
```

Artifacts land under `src-tauri/target/release/bundle/`:

- `macos/Flex.app` — launch directly (`open …/Flex.app`)
- `dmg/Flex_*_aarch64.dmg` — drag-to-Applications installer
- `nsis/Flex_*_x64-setup.exe` — Windows NSIS installer

Builds are unsigned until Apple Developer certs are wired (see
`.github/workflows/release.yml`). Tag `v*` pushes draft a GitHub Release with
the `.dmg`. For a full target set: `pnpm tauri build`.

## Configure a provider

1. Open **Settings** in the sidebar footer (or the welcome screen on first launch).
2. Choose a provider (`anthropic`, `openai`, `gemini`, `ollama`, …).
3. Paste an API key (skipped for Ollama). Optionally set a base URL / host and
   default model.
4. **Validate** then **Save**. Secrets are encrypted at rest (AES-256-GCM) in a
   local file by default; the macOS Keychain is opt-in per the storage-mode
   setting. Never `localStorage`.
5. Pick a project folder from the chat context bar (tools are sandboxed to that cwd).

After save, create a session (**New Agent**), pick a model + reasoning effort in
the composer, attach files/images via **+** (clipboard paste works), and send.
Turns stream inline with markdown, thinking, and clustered tool steps.

Optional engine settings (same form): enable **Search** / **Learning** / **Verifier**
plugins, install **MCP servers** from the catalog, set default + **fallback
models** per connection, and choose default workspace **isolation** (Direct or
Isolated). Type `/` in the composer for slash-command autocomplete.

## UI notes

- Agents-first window chrome: recessed sidebar, content canvas, light/dark
  themes (toggle in sidebar footer), flat expanded composer (12px radius, quiet
  toolbar).
- Sidebar: **New Agent**, **Search** (⌘K); agents grouped by repository with
  running dots. Sessions get a semantic auto-title after the first turn. Settings
  + theme live in the footer. New Agent prefers the latest project from the picker.
- Persistent sidebar across chat ↔ settings; content pane fades softly.
- Composer modes: **Agent** / **Plan** / **Ask** → engine `PermissionMode`, a
  per-model reasoning-effort picker, and a circular **bypass shield** next to send
  for per-session permission bypass. **Flex** mode adds cross-provider tier
  orchestration (planner → independent reviewer → isolated workers).
- Per-session drafts; Enter sends (Shift+Enter newline).
- Timeline: streaming thinking, live-stacking tool clusters ("Edited 3 files"),
  an always-on work indicator, and completed turns fold into a "Worked for Ns"
  summary with duration, tokens, and cost. `plan_updated` → `PlanCard`;
  `question_requested` → a step-by-step wizard docked into the composer.
- Right panel: on-demand, closable preview tabs (Changes / Browser / Terminal /
  Files) with a collapsible slim strip. **Changes** shows per-file ± counts and a
  commit center — select files, write a message, and Commit / Commit & Push /
  Create Branch & Commit / Commit & Create PR (`gh`, with graceful degradation).
- Background bash: long commands can move to the background and keep streaming;
  killing one terminates its whole process group.
- Resilience: transient provider errors retry on an escalating backoff with a
  reconnect banner; turns survive brief network loss.
- Motion: 100–160ms hover/tray/end-of-turn; `prefers-reduced-motion` respected.

## Architecture (short)

| Layer | Responsibility |
|---|---|
| React UI | Atomic Design components + Zustand / TanStack Query |
| `src-tauri` | Thin `#[tauri::command]` shell, keychain I/O, event fan-out |
| `agentloop-sdk` | `AgentBuilder` → `EngineService` (sole composition path) |
| `agentloop-engine` | Sessions, prompt, cancel, subscribe |

See [COMPONENTS.md](./COMPONENTS.md) for the Atomic component catalog.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| Enter | Send prompt (Shift+Enter for newline) |
| ⌘/Ctrl+Enter | Send prompt |
| ⌘/Ctrl+N | New session |
| ⌘/Ctrl+K | Toggle agent search |
| ⌘/Ctrl+L | Focus composer |
| Esc | Cancel in-flight turn |

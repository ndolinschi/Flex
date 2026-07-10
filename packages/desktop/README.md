# Desktop — Tauri UI for the agent-loop engine

In-process Tauri 2 + React desktop shell. Composes the engine via
`agentloop-sdk` (`AgentBuilder`) — the same composition root as the `flex` CLI.
Basic native agent loop only: preferred provider, no plugins, no connectors.

## Prerequisites

- Rust toolchain (see repo root)
- Node 20+
- Platform deps for [Tauri 2](https://v2.tauri.app/start/prerequisites/)

## Run

```bash
cd packages/desktop
npm install
npm run tauri dev
```

Browser UI preview (no Tauri):

```bash
npm run dev
```

Production build:

```bash
npm run tauri build
```

## Configure a provider

1. Open **Settings** in the sidebar footer (or the welcome screen on first launch).
2. Choose a provider (`anthropic`, `openai`, `gemini`, `ollama`, …).
3. Paste an API key (skipped for Ollama). Optionally set a base URL / host and
   default model.
4. **Validate** then **Save**. Keys go to the OS keychain — never `localStorage`.
5. Pick a project folder from the chat context bar (tools are sandboxed to that cwd).

After save, create a session (**New Agent**), pick a model in the composer, attach
files/images via **+**, and send. Turns stream inline with markdown rendering.

Optional engine settings (same form): enable **Search** / **Learning** / **Verifier**
plugins, set comma-separated **fallback models**, and choose default workspace
**isolation**. Type `/` in the composer for slash-command autocomplete.

## UI notes

- Cursor Agents Window chrome: recessed sidebar, content canvas, light/dark themes
  (toggle in sidebar footer), flat expanded composer (12px radius, quiet toolbar).
- Sidebar: **New Agent**, **Search** (⌘K); agents grouped by repository with
  running dots. Settings + theme live in the footer. New Agent prefers the latest
  project from the picker when available.
- Persistent sidebar across chat ↔ settings; content pane fades softly.
- Composer modes: **Agent** / **Plan** / **Ask** → engine `PermissionMode`.
- Per-session drafts; Enter sends (Shift+Enter newline).
- Timeline: `plan_updated` (`PlanCard`), `question_requested` (`QuestionPrompt`),
  Cursor-style work groups and clustered tool steps.
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

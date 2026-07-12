import type { UnlistenFn } from "@tauri-apps/api/event"
import type {
  BackgroundProcessDto,
  BrowserStateEvent,
  BuiltinProvider,
  CreateSessionInput,
  GitFileStatus,
  GitStatusSummary,
  McpServerDto,
  MemoryEntryDto,
  ModelInfoDto,
  PromptCommandInput,
  ProviderConfigView,
  ProviderProfileInput,
  ProviderProfileView,
  RoutineDto,
  RoutineRunRecordDto,
  SecretStorageMode,
  SessionEvent,
  SessionMeta,
  TerminalExitEvent,
  TerminalInfo,
  TerminalOutputEvent,
  ToolCall,
  TurnSummary,
  UpdateSessionInput,
} from "./types"

const now = () => Date.now()

const demoSession = (overrides: Partial<SessionMeta> = {}): SessionMeta => ({
  id: "preview-session-1",
  title: "Chat shell redesign",
  agent_id: "native",
  depth: 0,
  cwd: "/Users/preview/flex-app",
  model: "anthropic/claude-sonnet-4",
  fallback_models: [],
  created_at_ms: now() - 3_600_000,
  updated_at_ms: now() - 120_000,
  ...overrides,
})

/** Cwd for the one demo session deliberately mocked as NOT a git repo (see
 * `git_is_repo` mock below) — lets the preview demonstrate the "hide the
 * entire git cluster" behavior without a real non-git folder on disk. */
const NON_GIT_DEMO_CWD = "/Users/preview/scratch-notes"

/** Cwd for the demo session mocked with a large synthetic Changes list (see
 * `mockLargeGitStatus` below) — lets the preview demonstrate the Changes
 * panel's row cap + "+N more" overflow indicator without needing to actually
 * scaffold hundreds of real files on disk (e.g. `create-next-app`'s
 * `node_modules`/`.next` tree). */
const LARGE_CHANGES_DEMO_CWD = "/Users/preview/scaffolded-next-app"

let sessions: SessionMeta[] = [
  demoSession(),
  demoSession({
    id: "preview-session-2",
    title: "Empty session hero",
    updated_at_ms: now() - 60_000,
  }),
  demoSession({
    id: "preview-session-3",
    title: "Docs restructure",
    cwd: "/Users/preview/docs-site",
    created_at_ms: now() - 10_000_000,
    updated_at_ms: now() - 7_200_000,
  }),
  demoSession({
    id: "preview-session-4",
    title: "Scratch notes (no git)",
    cwd: NON_GIT_DEMO_CWD,
    created_at_ms: now() - 5_000_000,
    updated_at_ms: now() - 4_800_000,
  }),
  demoSession({
    id: "preview-session-5",
    title: "Interrupted question",
    cwd: "/Users/preview/flex-app",
    created_at_ms: now() - 8_000_000,
    updated_at_ms: now() - 6_500_000,
  }),
  demoSession({
    id: "preview-session-6",
    title: "Realistic interleaved turn",
    cwd: "/Users/preview/flex-app",
    created_at_ms: now() - 400_000,
    updated_at_ms: now() - 300_000,
  }),
  demoSession({
    id: "preview-session-7",
    title: "Scaffolded Next.js app",
    cwd: LARGE_CHANGES_DEMO_CWD,
    created_at_ms: now() - 200_000,
    updated_at_ms: now() - 60_000,
  }),
  demoSession({
    id: "preview-session-8",
    title: "Narrated multi-tool turn",
    cwd: "/Users/preview/flex-app",
    created_at_ms: now() - 500_000,
    updated_at_ms: now() - 250_000,
  }),
  demoSession({
    id: "preview-session-9",
    title: "Multi-iteration tool-result turn",
    cwd: "/Users/preview/flex-app",
    created_at_ms: now() - 600_000,
    updated_at_ms: now() - 350_000,
  }),
]

let configured = true
const eventHandlers = new Set<(event: SessionEvent) => void>()
/** Demo background-process id seeded per session by the timeline fixture
 * (`bgCallId` below), so `background_list`/`background_kill` have something
 * consistent to report. */
const mockBackgroundIdBySession = new Map<string, string>()
const mockKilledBackgroundIds = new Set<string>()
/** Call ids that `background_demote` (see the invoke case below) has flipped
 * into the background presentation — lets the mock's own timers (which would
 * otherwise complete the demoted call normally) check and skip re-emitting a
 * plain "completed" result over top of the demote. */
const mockDemotedCallIds = new Set<string>()

/** Mutable so `review_undo_file`/`review_keep_file` can plausibly affect the
 * Changes tab in preview — undo removes a file from the list (mirroring the
 * real command reverting it to its base state, i.e. no more diff), keep is a
 * no-op on the list (the file stays "changed" relative to the *worktree*,
 * only the base repo's copy is updated). Kept as a mutable module-level
 * array (not sessionStorage) since it's only meant to demo one preview
 * session's flow, not survive a reload. */
let mockGitStatus: GitFileStatus[] = [
  { path: "src/App.tsx", status: "M", added: 24, removed: 3 },
  { path: "src/styles/tokens.css", status: "M", added: 58, removed: 12 },
  { path: "src/pages/CustomizePage.tsx", status: "?" },
]

/** Synthetic "many changed files" list for `LARGE_CHANGES_DEMO_CWD` — mirrors
 * what a real session looks like after scaffolding a Next.js app (hundreds of
 * brand-new untracked files under `node_modules`/`.next`/generated code).
 * Used to exercise the Changes panel's row cap + "+N more" overflow indicator
 * in preview, matching the server-side `MAX_STATUS_FILES` cap in
 * `commands.rs` (kept in sync below as `MOCK_MAX_STATUS_FILES`). */
const MOCK_MAX_STATUS_FILES = 300
const mockLargeGitStatus: GitFileStatus[] = Array.from(
  { length: 400 },
  (_, i) => ({
    path: `node_modules/some-pkg-${i}/index.js`,
    status: "?",
  }),
)

/** Builds a `GitStatusSummary` from a mock file list, applying the same
 * row cap + totals-over-full-set behavior as the real `summarize` in
 * `commands.rs` — keeps the mock's shape (and cap behavior) honest with the
 * real backend so the preview is a faithful stand-in for perf testing. */
const mockStatusSummary = (files: GitFileStatus[]): GitStatusSummary => {
  const totalAdded = files.reduce((sum, f) => sum + (f.added ?? 0), 0)
  const totalRemoved = files.reduce((sum, f) => sum + (f.removed ?? 0), 0)
  return {
    files: files.slice(0, MOCK_MAX_STATUS_FILES),
    totalCount: files.length,
    totalAdded,
    totalRemoved,
    truncated: files.length > MOCK_MAX_STATUS_FILES,
  }
}

/** Per-path mock diff bodies — `src/App.tsx` gets two hunks so the preview
 * demonstrates independent per-hunk Keep/Undo, not just per-file. */
const mockDiffFor = (p: string): string => {
  // Untracked file (Changes tab "U" badge) — mirrors the real
  // `git diff --no-index /dev/null <path>` fallback in
  // `diff_against_rev` (src-tauri/src/commands.rs): no prior baseline, so
  // the whole file renders as one all-green addition hunk.
  if (p === "src/pages/CustomizePage.tsx") {
    const lines = [
      "export const CustomizePage = () => {",
      "  return (",
      "    <div className=\"p-6\">",
      "      <h1>Customize</h1>",
      "    </div>",
      "  )",
      "}",
      "",
    ]
    return [
      `diff --git a/dev/null b/${p}`,
      "new file mode 100644",
      "index 0000000..8b1d4e7",
      "--- /dev/null",
      `+++ b/${p}`,
      `@@ -0,0 +1,${lines.length - 1} @@`,
      ...lines.slice(0, -1).map((l) => `+${l}`),
      "",
    ].join("\n")
  }
  if (p === "src/App.tsx") {
    return [
      `diff --git a/${p} b/${p}`,
      `index 3f9c2a1..8b1d4e7 100644`,
      `--- a/${p}`,
      `+++ b/${p}`,
      "@@ -12,7 +12,9 @@ export const AppRoutes = () => {",
      "   const route = useAppStore((s) => s.route)",
      "   const isBootstrapped = useAppStore((s) => s.isBootstrapped)",
      "-  const setRoute = useAppStore((s) => s.setRoute)",
      "+  const setRoute = useAppStore((s) => s.setRoute)",
      "+  const toggleRightPanel = useAppStore((s) => s.toggleRightPanel)",
      "+  const rightPanelOpen = useAppStore((s) => s.rightPanelOpen)",
      "   const { newAgent } = useSessions()",
      "   useGlobalSessionEvents()",
      "@@ -40,6 +42,8 @@ export const AppRoutes = () => {",
      "   return (",
      "     <ChatShell>",
      "-      <LegacyHeader />",
      "+      <Header />",
      "+      <StatusBar />",
      "     </ChatShell>",
      "   )",
      " }",
      "",
    ].join("\n")
  }
  return [
    `diff --git a/${p} b/${p}`,
    `index 3f9c2a1..8b1d4e7 100644`,
    `--- a/${p}`,
    `+++ b/${p}`,
    "@@ -1,4 +1,6 @@",
    " :root {",
    "-  --accent: #444;",
    "+  --accent: #3366ff;",
    "+  --accent-hover: #2952cc;",
    "   --bg: #0b0b0c;",
    " }",
    "",
  ].join("\n")
}

let mockBranchByCwd: Record<string, string> = {
  "/Users/preview/flex-app": "main",
  "/Users/preview/docs-site": "main",
}
const mockBranches = ["main", "desktop/chat-redesign", "engine/opencode-gap"]

/** Representative file tree for the composer @-mention popover in preview. */
const mockFiles = [
  "README.md",
  "package.json",
  "src/App.tsx",
  "src/index.css",
  "src/main.tsx",
  "src/components/organisms/Composer.tsx",
  "src/components/organisms/SessionSidebar.tsx",
  "src/components/organisms/TurnTimeline.tsx",
  "src/components/molecules/WorkGroup.tsx",
  "src/components/molecules/ToolStepGroup.tsx",
  "src/hooks/useSessionEvents.ts",
  "src/hooks/useKeyboardShortcuts.ts",
  "src/lib/tauri.ts",
  "src/lib/types.ts",
  "src/stores/appStore.ts",
  "src/styles/tokens.css",
]

const models: ModelInfoDto[] = [
  {
    id: "anthropic/claude-sonnet-4",
    providerId: "anthropic",
    displayName: "Claude Sonnet 4",
    contextWindow: 200_000,
  },
  {
    id: "openai/gpt-4.1",
    providerId: "openai",
    displayName: "GPT-4.1",
    contextWindow: 1_000_000,
  },
]

/** Mutable so save_provider_config round-trips plugin toggles in preview;
    kept in sessionStorage so the state survives a page reload. */
const loadMockPlugins = (): {
  search: boolean
  index: boolean
  autoContext: boolean
  learning: boolean
  verifier: boolean
} => {
  try {
    const raw = window.sessionStorage.getItem("mock-plugins")
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<ReturnType<typeof loadMockPlugins>>
      return {
        search: parsed.search ?? true,
        index: parsed.index ?? true,
        autoContext: parsed.autoContext ?? false,
        learning: parsed.learning ?? true,
        verifier: parsed.verifier ?? true,
      }
    }
  } catch {
    // Fall through to defaults.
  }
  // Desktop enables [search, index, learning, verifier] by default in preview
  // so the Verify tool call / verdict demo (see timelineEvents + the `verify`
  // prompt keyword below) is visible without opening Customize first.
  return {
    search: true,
    index: true,
    autoContext: false,
    learning: true,
    verifier: true,
  }
}

let mockPlugins = loadMockPlugins()

/** Seed demo routines — kept in sessionStorage so upsert/remove round-trip
    across a page reload, mirroring the mockPlugins pattern above. */
const defaultMockRoutines = (): RoutineDto[] => [
  {
    id: "nightly-review",
    prompt: "Review overnight PRs opened against main, summarize risk, and flag anything that needs a human look before merge.",
    maxIterations: 8,
    maxIdenticalFailures: 3,
    requireVerification: true,
    trigger: { kind: "cron", expr: "0 9 * * *" },
    title: "Nightly PR review",
  },
  {
    id: "deploy-check",
    prompt: "Run the deploy health checklist against the last release and report any failing checks.",
    maxIterations: 8,
    maxIdenticalFailures: 3,
    requireVerification: false,
    trigger: { kind: "webhook", path: "/deploy" },
    title: "Deploy health check",
  },
]

const loadMockRoutines = (): RoutineDto[] => {
  try {
    const raw = window.sessionStorage.getItem("mockRoutines")
    if (raw) return JSON.parse(raw) as RoutineDto[]
  } catch {
    // Fall through to defaults.
  }
  return defaultMockRoutines()
}

let mockRoutines = loadMockRoutines()

const saveMockRoutines = () => {
  try {
    window.sessionStorage.setItem("mockRoutines", JSON.stringify(mockRoutines))
  } catch {
    // Non-fatal in preview.
  }
}

const loadMockRoutineHistory = (): Record<string, RoutineRunRecordDto[]> => {
  try {
    const raw = window.sessionStorage.getItem("mockRoutineHistory")
    if (raw) return JSON.parse(raw) as Record<string, RoutineRunRecordDto[]>
  } catch {
    // Fall through to defaults.
  }
  return {}
}

let mockRoutineHistory = loadMockRoutineHistory()

const saveMockRoutineHistory = () => {
  try {
    window.sessionStorage.setItem(
      "mockRoutineHistory",
      JSON.stringify(mockRoutineHistory),
    )
  } catch {
    // Non-fatal in preview.
  }
}

/** Seed demo MCP servers — kept in sessionStorage so upsert/remove round-trip
    across a page reload, mirroring the mockRoutines pattern above. */
const defaultMockMcpServers = (): McpServerDto[] => [
  {
    id: "filesystem",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-filesystem", "/Users/preview/flex-app"],
    env: {},
    enabled: true,
  },
  {
    id: "github",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-github"],
    env: { GITHUB_PERSONAL_ACCESS_TOKEN: "" },
    enabled: false,
  },
]

const loadMockMcpServers = (): McpServerDto[] => {
  try {
    const raw = window.sessionStorage.getItem("mockMcpServers")
    if (raw) return JSON.parse(raw) as McpServerDto[]
  } catch {
    // Fall through to defaults.
  }
  return defaultMockMcpServers()
}

let mockMcpServers = loadMockMcpServers()

const saveMockMcpServers = () => {
  try {
    window.sessionStorage.setItem("mockMcpServers", JSON.stringify(mockMcpServers))
  } catch {
    // Non-fatal in preview.
  }
}

/** Seed demo memories — kept in sessionStorage so remove round-trips across a
    page reload, mirroring the mockRoutines pattern above. */
const defaultMockMemories = (): MemoryEntryDto[] => [
  {
    id: "user-preferences",
    title: "User preferences",
    content:
      "- Prefers concise commit messages in the imperative mood.\n" +
      "- Likes vim keybindings in the terminal panel.\n" +
      "- Answers should default to TypeScript over JavaScript for new files.",
    updatedAtMs: now() - 3_600_000,
  },
  {
    id: "project-facts",
    title: "Project facts",
    content:
      "- This repo is a Tauri + React desktop app talking to a Rust engine over IPC.\n" +
      "- The mock IPC layer lives in `src/lib/browserMock.ts` for browser preview.\n" +
      "- Verify changes with `npx tsc --noEmit` before considering a task done.",
    updatedAtMs: now() - 26 * 3_600_000,
  },
  {
    id: "deploy-quirks",
    title: "Deploy quirks",
    content:
      "- Staging deploys require a manual cache bust after asset changes.\n" +
      "- The release checklist lives in the automations page, not in this repo.",
    updatedAtMs: now() - 4 * 24 * 3_600_000,
  },
]

const loadMockMemories = (): MemoryEntryDto[] => {
  try {
    const raw = window.sessionStorage.getItem("mockMemories")
    if (raw) return JSON.parse(raw) as MemoryEntryDto[]
  } catch {
    // Fall through to defaults.
  }
  return defaultMockMemories()
}

let mockMemories = loadMockMemories()

const saveMockMemories = () => {
  try {
    window.sessionStorage.setItem("mockMemories", JSON.stringify(mockMemories))
  } catch {
    // Non-fatal in preview.
  }
}

/** Mock sidecar `expiry.json` for global memory — `<id, expiresAtMs>`,
 * mirroring the real `expiry.json` file the desktop backend keeps next to
 * the `.md` notes (see `src-tauri/src/commands.rs`'s "Memory expiry"
 * section). Kept in sessionStorage so a set TTL / purge round-trips across
 * a page reload, same as the mockMemories pattern above. */
const loadMockMemoryExpiry = (): Record<string, number> => {
  try {
    const raw = window.sessionStorage.getItem("mockMemoryExpiry")
    if (raw) return JSON.parse(raw) as Record<string, number>
  } catch {
    // Fall through to defaults.
  }
  return {}
}

let mockMemoryExpiry = loadMockMemoryExpiry()

const saveMockMemoryExpiry = () => {
  try {
    window.sessionStorage.setItem("mockMemoryExpiry", JSON.stringify(mockMemoryExpiry))
  } catch {
    // Non-fatal in preview.
  }
}

/** Delete any global memory entries whose mock expiry has passed, mirroring
 * `purge_expired_memories` on the desktop backend. Called at the top of the
 * `memory_list` mock so listing is self-cleaning, same as the real command. */
const purgeMockMemories = () => {
  const nowMs = now()
  const expiredIds = Object.entries(mockMemoryExpiry)
    .filter(([, ts]) => ts <= nowMs)
    .map(([id]) => id)
  if (expiredIds.length === 0) return
  mockMemories = mockMemories.filter((m) => !expiredIds.includes(m.id))
  for (const id of expiredIds) delete mockMemoryExpiry[id]
  saveMockMemories()
  saveMockMemoryExpiry()
}

/** Seed per-project memory — keyed by session cwd so the Memory page's
    project sections have something to show in preview. Only the demo
    sessions' cwds get entries here; other projects legitimately render no
    section (mirroring an empty `<cwd>/.agent/memory` dir on disk). */
const defaultMockProjectMemories = (): Record<string, MemoryEntryDto[]> => ({
  "/Users/preview/flex-app": [
    {
      id: "chat-shell-notes",
      title: "Chat shell notes",
      content:
        "- The redesign keeps the composer pinned to the bottom on all viewport widths.\n" +
        "- Tool call cards collapse by default; expand state isn't persisted yet.",
      updatedAtMs: now() - 2 * 3_600_000,
    },
    {
      id: "test-fixtures",
      title: "Test fixtures",
      content:
        "- Golden snapshots for this repo live under `packages/desktop/src/**/__snapshots__`.\n" +
        "- Re-run `npx tsc --noEmit` after touching shared types.",
      updatedAtMs: now() - 3 * 24 * 3_600_000,
    },
  ],
})

const loadMockProjectMemories = (): Record<string, MemoryEntryDto[]> => {
  try {
    const raw = window.sessionStorage.getItem("mockProjectMemories")
    if (raw) return JSON.parse(raw) as Record<string, MemoryEntryDto[]>
  } catch {
    // Fall through to defaults.
  }
  return defaultMockProjectMemories()
}

let mockProjectMemories = loadMockProjectMemories()

const saveMockProjectMemories = () => {
  try {
    window.sessionStorage.setItem(
      "mockProjectMemories",
      JSON.stringify(mockProjectMemories),
    )
  } catch {
    // Non-fatal in preview.
  }
}

/** Mock sidecar expiry map for project memory, keyed by `cwd` then `id` —
 * same rationale as `mockMemoryExpiry` above. */
const loadMockProjectMemoryExpiry = (): Record<string, Record<string, number>> => {
  try {
    const raw = window.sessionStorage.getItem("mockProjectMemoryExpiry")
    if (raw) return JSON.parse(raw) as Record<string, Record<string, number>>
  } catch {
    // Fall through to defaults.
  }
  return {}
}

let mockProjectMemoryExpiry = loadMockProjectMemoryExpiry()

const saveMockProjectMemoryExpiry = () => {
  try {
    window.sessionStorage.setItem(
      "mockProjectMemoryExpiry",
      JSON.stringify(mockProjectMemoryExpiry),
    )
  } catch {
    // Non-fatal in preview.
  }
}

/** Delete any expired entries from one project's mock memory, mirroring
 * `purgeMockMemories` above. */
const purgeMockProjectMemories = (cwd: string) => {
  const expiry = mockProjectMemoryExpiry[cwd] ?? {}
  const nowMs = now()
  const expiredIds = Object.entries(expiry)
    .filter(([, ts]) => ts <= nowMs)
    .map(([id]) => id)
  if (expiredIds.length === 0) return
  mockProjectMemories = {
    ...mockProjectMemories,
    [cwd]: (mockProjectMemories[cwd] ?? []).filter((m) => !expiredIds.includes(m.id)),
  }
  const nextExpiry = { ...expiry }
  for (const id of expiredIds) delete nextExpiry[id]
  mockProjectMemoryExpiry = { ...mockProjectMemoryExpiry, [cwd]: nextExpiry }
  saveMockProjectMemories()
  saveMockProjectMemoryExpiry()
}

/** Mutable so `set_secret_storage` round-trips in preview; kept in
 * sessionStorage so the choice survives a page reload, mirroring the
 * mockPlugins pattern above. Defaults to `"file"` — the new default. */
const loadMockSecretStorage = (): SecretStorageMode => {
  try {
    const raw = window.sessionStorage.getItem("mock-secret-storage")
    if (raw === "file" || raw === "keychain") return raw
  } catch {
    // Fall through to default.
  }
  return "file"
}

let mockSecretStorage: SecretStorageMode = loadMockSecretStorage()

const providerConfig = (): ProviderConfigView => ({
  preferredProvider: "anthropic",
  baseUrl: "https://api.anthropic.com",
  defaultModel: "anthropic/claude-sonnet-4",
  cwd: "/Users/preview/flex-app",
  configuredProviders: ["anthropic"],
  hasAnyKey: true,
  plugins: { ...mockPlugins },
  fallbackModels: [],
  defaultIsolation: "never",
  secretStorage: mockSecretStorage,
})

/** Seed one "Anthropic" connection — mirrors the mockRoutines pattern.
 * `hasKey`/`isActive` are derived at read time (see `mockProfilesView`), not
 * stored, since "active" is a separate id pointer that can move. */
type MockProfile = {
  id: string
  label: string
  provider: string
  baseUrl?: string
  region?: string
  defaultModel?: string
  fallbackModels?: string
  defaultIsolation?: string
  hasKey: boolean
}

const defaultMockProfiles = (): MockProfile[] => [
  {
    id: "default",
    label: "Anthropic",
    provider: "anthropic",
    baseUrl: "https://api.anthropic.com",
    defaultModel: "anthropic/claude-sonnet-4",
    defaultIsolation: "never",
    hasKey: true,
  },
]

const loadMockProfiles = (): MockProfile[] => {
  try {
    const raw = window.sessionStorage.getItem("mockProfiles")
    if (raw) return JSON.parse(raw) as MockProfile[]
  } catch {
    // Fall through to defaults.
  }
  return defaultMockProfiles()
}

let mockProfiles = loadMockProfiles()

const saveMockProfiles = () => {
  try {
    window.sessionStorage.setItem("mockProfiles", JSON.stringify(mockProfiles))
  } catch {
    // Non-fatal in preview.
  }
}

const loadMockActiveProfileId = (): string => {
  try {
    return window.sessionStorage.getItem("mockActiveProfileId") ?? "default"
  } catch {
    return "default"
  }
}

let mockActiveProfileId = loadMockActiveProfileId()

const saveMockActiveProfileId = () => {
  try {
    window.sessionStorage.setItem("mockActiveProfileId", mockActiveProfileId)
  } catch {
    // Non-fatal in preview.
  }
}

const mockProfileView = (p: MockProfile): ProviderProfileView => ({
  id: p.id,
  label: p.label,
  provider: p.provider,
  baseUrl: p.baseUrl,
  region: p.region,
  defaultModel: p.defaultModel,
  fallbackModels: p.fallbackModels,
  defaultIsolation: p.defaultIsolation,
  hasKey: p.hasKey,
  isActive: p.id === mockActiveProfileId,
})

let mockProfileSeq = 0

/** In-flight mock turns — cancel clears timers and emits turn_completed. */
const pendingTurns = new Map<
  string,
  { timers: number[]; turnId: string; startedAt: number }
>()

/**
 * Real-world teardown delay a `cancel()` call introduces before the engine's
 * turn task actually observes its `CancellationToken` and releases the
 * per-session `turn_gate` (see `NativeAgent::cancel`/`prompt` in
 * `packages/engine/crates/loop/src/agent.rs`): `cancel()` only flips the
 * token and returns immediately — the turn keeps holding the gate until its
 * next `tokio::select!` checkpoint. A resend that races into that window hits
 * `AgentError::TurnInProgress` (`TURN_IN_PROGRESS_MARKER`). Mocked here so the
 * preview can exercise the exact stop -> resend race instead of a cancel that
 * (unrealistically) tears down the turn synchronously.
 */
const MOCK_CANCEL_TEARDOWN_MS = 400

const emit = (event: SessionEvent) => {
  for (const handler of eventHandlers) handler(event)
}

const clearPendingTurn = (sessionId: string) => {
  const pending = pendingTurns.get(sessionId)
  if (!pending) return
  for (const id of pending.timers) window.clearTimeout(id)
  pendingTurns.delete(sessionId)
}

const demoEditCall = (
  sessionId: string,
  overrides: Partial<ToolCall> & {
    input?: Record<string, unknown>
  } = {},
): ToolCall => {
  const { input: inputOverride, ...rest } = overrides
  return {
    id: "tool-1",
    session_id: sessionId,
    turn_id: "turn-1",
    message_id: "m-asst-1",
    tool_name: "Edit",
    input: {
      file_path: "packages/desktop/src/components/organisms/Composer.tsx",
      old_string: "a\nb\nc\nd",
      new_string: "a\nb\nc\nd\ne\nf\ng\nh\ni\nj",
      ...inputOverride,
    },
    read_only: false,
    origin: { origin: "model" },
    status: { state: "completed" },
    timing: {
      queued_at_ms: now() - 80_000,
      started_at_ms: now() - 79_000,
      finished_at_ms: now() - 78_000,
    },
    result: {
      content: [
        {
          type: "markdown",
          text: "Edited file (1 replacement).",
        },
      ],
      is_error: false,
    },
    ...rest,
  }
}

const demoShellCall = (sessionId: string, command: string, id: string): ToolCall => ({
  id,
  session_id: sessionId,
  turn_id: "turn-1",
  message_id: "m-asst-1",
  tool_name: "Bash",
  input: { command },
  read_only: false,
  origin: { origin: "model" },
  status: { state: "completed" },
  timing: {
    queued_at_ms: now() - 70_000,
    started_at_ms: now() - 69_000,
    finished_at_ms: now() - 68_000,
  },
  result: {
    content: [{ type: "markdown", text: "exit_code: 0\n\nstdout:\nok" }],
    is_error: false,
    structured: { exit_code: 0, success: true, truncated: false },
  },
})

const demoReadCall = (
  sessionId: string,
  filePath: string,
  id: string,
  offset = 1,
  limit = 40,
): ToolCall => ({
  id,
  session_id: sessionId,
  turn_id: "turn-1",
  message_id: "m-asst-1",
  tool_name: "Read",
  input: { file_path: filePath, offset, limit },
  read_only: true,
  origin: { origin: "model" },
  status: { state: "completed" },
  timing: {
    queued_at_ms: now() - 75_000,
    started_at_ms: now() - 74_000,
    finished_at_ms: now() - 73_000,
  },
  result: {
    content: [{ type: "markdown", text: `Read ${filePath}` }],
    is_error: false,
    structured: {
      file_path: filePath,
      shown_lines: limit,
      total_lines: 400,
    },
  },
})

const demoExploreCall = (sessionId: string): ToolCall => ({
  id: "tool-0",
  session_id: sessionId,
  turn_id: "turn-1",
  message_id: "m-asst-1",
  tool_name: "Glob",
  input: { pattern: "**/*.{tsx,css}" },
  read_only: true,
  origin: { origin: "model" },
  status: { state: "completed" },
  timing: {
    queued_at_ms: now() - 85_000,
    started_at_ms: now() - 84_000,
    finished_at_ms: now() - 83_000,
  },
  result: {
    content: [{ type: "markdown", text: "Found 8 files matching pattern." }],
    is_error: false,
    structured: {
      count: 8,
      files: [
        "packages/desktop/src/App.tsx",
        "packages/desktop/src/index.css",
        "packages/desktop/src/styles/tokens.css",
        "packages/desktop/src/components/organisms/Composer.tsx",
        "packages/desktop/src/components/organisms/TurnTimeline.tsx",
        "packages/desktop/src/components/molecules/ToolCallChip.tsx",
        "packages/desktop/src/components/templates/ChatShell.tsx",
        "packages/desktop/src/pages/ChatPage.tsx",
      ],
    },
  },
})

/** A completed `Verify` call — the wire shape a real engine emits from
 * `EngineService::verify_goal_progress` (packages/engine/crates/engine/src/lib.rs)
 * during a `run_goal` iteration with `GoalSpec.require_verification: true`.
 * `result.structured` is the load-bearing field: a `VerificationVerdict`
 * (`{ outcome, findings, confidence? }`) — see `agentloop_contracts::verify`. */
const demoVerifyCall = (
  sessionId: string,
  overrides: Partial<ToolCall> = {},
): ToolCall => ({
  id: "tool-verify-1",
  session_id: sessionId,
  turn_id: "turn-1",
  message_id: "m-asst-1",
  tool_name: "Verify",
  input: {
    rubric: "The chat shell spacing matches the reference agents window design.",
    artifacts: [
      "packages/desktop/src/components/organisms/TurnTimeline.tsx",
      "packages/desktop/src/components/molecules/WorkGroup.tsx",
    ],
  },
  read_only: true,
  origin: { origin: "model" },
  status: { state: "completed" },
  timing: {
    queued_at_ms: now() - 62_000,
    started_at_ms: now() - 61_000,
    finished_at_ms: now() - 58_000,
  },
  result: {
    content: [
      {
        type: "markdown",
        text: "Verdict: pass — spacing and hierarchy match the target design.",
      },
    ],
    is_error: false,
    structured: {
      outcome: "pass",
      findings: [
        "Composer rail and timeline share the same --content-rail width.",
        "Work groups collapse to a single summary line matching the reference density.",
      ],
      confidence: 0.92,
    },
  },
  ...overrides,
})

const timelineEvents = (sessionId: string): SessionEvent[] => {
  if (sessionId === "preview-session-2") return []
  const ts = now() - 90_000

  // The demo subagent child session — replayed by the subagent viewer tray.
  if (sessionId === "preview-sub-1") {
    return [
      {
        session_id: sessionId,
        seq: 1,
        ts_ms: ts,
        payload: {
          kind: "user_message",
          message_id: "m-sub-user",
          content: [
            {
              type: "markdown",
              text: "Map how the timeline renders subagent events.",
            },
          ],
        },
      },
      {
        session_id: sessionId,
        seq: 2,
        ts_ms: ts + 400,
        payload: { kind: "turn_started", turn_id: "t-sub-1" },
      },
      {
        session_id: sessionId,
        seq: 3,
        ts_ms: ts + 900,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoExploreCall(sessionId),
            id: "sub-call-1",
            status: { state: "completed" },
          },
        },
      },
      {
        session_id: sessionId,
        seq: 4,
        ts_ms: ts + 1_600,
        payload: {
          kind: "assistant_message",
          message_id: "m-sub-answer",
          content: [
            {
              type: "markdown",
              text:
                "The timeline nests subagent events under a collapsible " +
                "group; each child row reuses the same tool-step components " +
                "as the parent feed.",
            },
          ],
        },
      },
      {
        session_id: sessionId,
        seq: 5,
        ts_ms: ts + 1_700,
        payload: {
          kind: "turn_completed",
          turn_id: "t-sub-1",
          summary: {
            turn_id: "t-sub-1",
            stop_reason: "end_turn",
            usage: { input: 4_200, output: 310 },
            num_model_calls: 1,
            num_tool_calls: 1,
            duration_ms: 1_300,
          },
        },
      },
    ]
  }

  // Realistic-interleaving coverage for the "clustering fails on real data" /
  // "completed turn stays open" bugs: the idealized scenarios above emit one
  // tool call at a time with strictly monotonic timestamps and no assistant
  // chatter between them, which never exercises `clusterToolRows`'
  // adjacency check the way a real engine run does. This mirrors the exact
  // confirmed-bug sequence — Bash, Edit, Edit, Bash — but with a real
  // session's actual shape: an empty-text thinking chunk (assistant_message
  // with `thinking` content only, no markdown yet — completely ordinary
  // mid-turn model output) landing between the two Edit calls, each tool
  // call's own `tool_call_updated` arriving as separate started→completed
  // events (not a single already-settled call), and the turn's final
  // `turn_completed` arriving after everything so folding must engage.
  if (sessionId === "preview-session-6") {
    return [
      {
        session_id: sessionId,
        seq: 1,
        ts_ms: ts,
        payload: {
          kind: "user_message",
          message_id: "m-user-6",
          content: [
            { type: "markdown", text: "Run the checks, fix both files, then rerun." },
          ],
        },
      },
      {
        session_id: sessionId,
        seq: 2,
        ts_ms: ts + 200,
        payload: { kind: "turn_started", turn_id: "turn-6" },
      },
      // Bash #1 — starts, then completes.
      {
        session_id: sessionId,
        seq: 3,
        ts_ms: ts + 400,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoShellCall(sessionId, "npx tsc --noEmit", "call-bash-1"),
            turn_id: "turn-6",
            status: { state: "running" },
            result: undefined,
          },
        },
      },
      {
        session_id: sessionId,
        seq: 4,
        ts_ms: ts + 900,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoShellCall(sessionId, "npx tsc --noEmit", "call-bash-1"),
            turn_id: "turn-6",
          },
        },
      },
      // Edit #1 — starts, then completes.
      {
        session_id: sessionId,
        seq: 5,
        ts_ms: ts + 1_100,
        payload: {
          kind: "tool_call_updated",
          call: demoEditCall(sessionId, {
            id: "call-edit-1",
            turn_id: "turn-6",
            status: { state: "running" },
            result: undefined,
            input: {
              file_path: "packages/desktop/src/components/organisms/TurnTimeline.tsx",
              old_string: "old-a",
              new_string: "new-a\nnew-a2",
            },
          }),
        },
      },
      {
        session_id: sessionId,
        seq: 6,
        ts_ms: ts + 1_600,
        payload: {
          kind: "tool_call_updated",
          call: demoEditCall(sessionId, {
            id: "call-edit-1",
            turn_id: "turn-6",
            input: {
              file_path: "packages/desktop/src/components/organisms/TurnTimeline.tsx",
              old_string: "old-a",
              new_string: "new-a\nnew-a2",
            },
          }),
        },
      },
      // Empty-text thinking chunk between the two edits — a real
      // assistant_message with thinking content but no markdown yet. Renders
      // as nothing (TimelineRowView returns null for blank thinking/assistant
      // text) but still sits in the row array between the two Edit rows.
      {
        session_id: sessionId,
        seq: 6.5,
        ts_ms: ts + 1_750,
        payload: {
          kind: "assistant_message",
          message_id: "m-asst-6a",
          model: "anthropic/claude-sonnet-4",
          content: [{ type: "thinking", text: "   " }],
        },
      },
      // Edit #2 — starts, then completes.
      {
        session_id: sessionId,
        seq: 7,
        ts_ms: ts + 1_900,
        payload: {
          kind: "tool_call_updated",
          call: demoEditCall(sessionId, {
            id: "call-edit-2",
            turn_id: "turn-6",
            status: { state: "running" },
            result: undefined,
            input: {
              file_path: "packages/desktop/src/components/molecules/ToolCallChip.tsx",
              old_string: "old-b",
              new_string: "new-b\nnew-b2\nnew-b3",
            },
          }),
        },
      },
      {
        session_id: sessionId,
        seq: 8,
        ts_ms: ts + 2_400,
        payload: {
          kind: "tool_call_updated",
          call: demoEditCall(sessionId, {
            id: "call-edit-2",
            turn_id: "turn-6",
            input: {
              file_path: "packages/desktop/src/components/molecules/ToolCallChip.tsx",
              old_string: "old-b",
              new_string: "new-b\nnew-b2\nnew-b3",
            },
          }),
        },
      },
      // Bash #2 — starts, then completes.
      {
        session_id: sessionId,
        seq: 9,
        ts_ms: ts + 2_600,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoShellCall(sessionId, "cargo check -p desktop", "call-bash-2"),
            turn_id: "turn-6",
            status: { state: "running" },
            result: undefined,
          },
        },
      },
      {
        session_id: sessionId,
        seq: 10,
        ts_ms: ts + 3_100,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoShellCall(sessionId, "cargo check -p desktop", "call-bash-2"),
            turn_id: "turn-6",
          },
        },
      },
      {
        session_id: sessionId,
        seq: 11,
        ts_ms: ts + 3_300,
        payload: {
          kind: "assistant_message",
          message_id: "m-asst-6b",
          model: "anthropic/claude-sonnet-4",
          content: [
            { type: "markdown", text: "Fixed both files; typecheck and cargo check are clean." },
          ],
        },
      },
      {
        session_id: sessionId,
        seq: 12,
        ts_ms: ts + 3_400,
        payload: {
          kind: "turn_completed",
          turn_id: "turn-6",
          summary: {
            turn_id: "turn-6",
            stop_reason: "end_turn",
            usage: { input: 12_000, output: 800 },
            num_model_calls: 1,
            num_tool_calls: 4,
            duration_ms: 3_200,
          },
        },
      },
    ]
  }

  // Reproduces the exact confirmed-live bug transcript: a real turn with
  // SHORT, VISIBLE assistant narration ("Good — the project uses plain
  // CommonJS…") interleaved between tool calls, mid-turn, before the final
  // answer. `preview-session-6` above only ever interleaves an EMPTY-text
  // thinking chunk (invisible either way), which never exercised the real
  // failure mode: while a turn is still STREAMING, `buildDisplayItems` used
  // to only fold non-final assistant rows into the work group for a
  // COMPLETED turn (gated on `!keepOpen`) — so mid-stream, any narration row
  // rendered as its own floating item below the still-open group instead of
  // staying tucked inside it (see `buildDisplayItems`'s `flush`). Separately,
  // `clusterToolRows` used to treat any non-empty assistant row as a hard
  // boundary, splitting "Read 2 files"/"Edited 2 files" into singleton rows
  // whenever narration sat between them (see `isNonBreakingRow`, née
  // `isInvisibleConnectiveRow`). Sequence: Ran 1 command, Read test.js, Read
  // roman.js, narration, Edited 1 file x2, Ran 1 command, final answer.
  if (sessionId === "preview-session-8") {
    return [
      {
        session_id: sessionId,
        seq: 1,
        ts_ms: ts,
        payload: {
          kind: "user_message",
          message_id: "m-user-8",
          content: [
            { type: "markdown", text: "Run the test suite and fix any failures." },
          ],
        },
      },
      {
        session_id: sessionId,
        seq: 2,
        ts_ms: ts + 200,
        payload: { kind: "turn_started", turn_id: "turn-8" },
      },
      // Ran 1 command.
      {
        session_id: sessionId,
        seq: 3,
        ts_ms: ts + 400,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoShellCall(sessionId, "npm test", "call-8-bash-1"),
            turn_id: "turn-8",
          },
        },
      },
      // Read test.js.
      {
        session_id: sessionId,
        seq: 4,
        ts_ms: ts + 900,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoReadCall(sessionId, "test.js", "call-8-read-1", 1, 24),
            turn_id: "turn-8",
          },
        },
      },
      // Read roman.js.
      {
        session_id: sessionId,
        seq: 5,
        ts_ms: ts + 1_300,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoReadCall(sessionId, "roman.js", "call-8-read-2", 1, 21),
            turn_id: "turn-8",
          },
        },
      },
      // Visible mid-turn narration — the model thinking out loud between tool
      // calls. This is the row that must NOT split the group or break
      // clustering.
      {
        session_id: sessionId,
        seq: 6,
        ts_ms: ts + 1_500,
        payload: {
          kind: "assistant_message",
          message_id: "m-asst-8a",
          model: "anthropic/claude-sonnet-4",
          content: [
            {
              type: "markdown",
              text: "Good — the project uses plain CommonJS, so I can fix both files without touching the module config.",
            },
          ],
        },
      },
      // Edited 1 file (roman.js).
      {
        session_id: sessionId,
        seq: 7,
        ts_ms: ts + 1_800,
        payload: {
          kind: "tool_call_updated",
          call: demoEditCall(sessionId, {
            id: "call-8-edit-1",
            turn_id: "turn-8",
            input: {
              file_path: "roman.js",
              old_string: "old-roman",
              new_string: "new-roman\nnew-roman2",
            },
          }),
        },
      },
      // Edited 1 file (test.js).
      {
        session_id: sessionId,
        seq: 8,
        ts_ms: ts + 2_100,
        payload: {
          kind: "tool_call_updated",
          call: demoEditCall(sessionId, {
            id: "call-8-edit-2",
            turn_id: "turn-8",
            input: {
              file_path: "test.js",
              old_string: "old-test",
              new_string: "new-test\nnew-test2",
            },
          }),
        },
      },
      // Ran 1 command (rerun).
      {
        session_id: sessionId,
        seq: 9,
        ts_ms: ts + 2_500,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoShellCall(sessionId, "npm test", "call-8-bash-2"),
            turn_id: "turn-8",
          },
        },
      },
      // Final answer of the turn.
      {
        session_id: sessionId,
        seq: 10,
        ts_ms: ts + 2_800,
        payload: {
          kind: "assistant_message",
          message_id: "m-asst-8b",
          model: "anthropic/claude-sonnet-4",
          content: [
            { type: "markdown", text: "15/15 tests passed after the fix." },
          ],
        },
      },
      {
        session_id: sessionId,
        seq: 11,
        ts_ms: ts + 2_900,
        payload: {
          kind: "turn_completed",
          turn_id: "turn-8",
          summary: {
            turn_id: "turn-8",
            stop_reason: "end_turn",
            usage: { input: 9_000, output: 600 },
            num_model_calls: 1,
            num_tool_calls: 5,
            duration_ms: 2_700,
          },
        },
      },
    ]
  }

  // Exact mirror of a REAL captured engine transcript (see
  // HANDOFF-OPUS.md / real-session.jsonl): within ONE turn, the engine feeds
  // each iteration's tool results back to the model as a `user_message`
  // whose `content` is entirely `tool_result` blocks — never a genuine human
  // turn. `preview-session-6`/`-8` above never emitted these (they only ever
  // used raw `tool_call_updated` events), so they could not reproduce the
  // "flat, unfolded, tools don't cluster across iterations" bug: the old
  // code mapped EVERY `user_message` to a `user` row, and `buildDisplayItems`
  // treats a `user` row as a hard turn/group boundary, so 3 fake "user
  // turns" fragmented what is actually one turn into 4 pieces. The fix (see
  // `hasVisibleUserContent` in `types/ui.ts`, applied in
  // `applyEvent.ts::applyEventToTimeline`'s `user_message` case) drops any
  // `user_message` with no visible markdown/image/file content instead of
  // materializing a row for it — so tool rows from iteration 1 and
  // iteration 2 sit ADJACENT in the row list and still cluster ("Read 2
  // files" instead of two singleton "Read" rows), and the whole turn folds
  // into one "Worked for Ns" group with only the final answer outside it.
  if (sessionId === "preview-session-9") {
    return [
      {
        session_id: sessionId,
        seq: 1,
        ts_ms: ts,
        payload: {
          kind: "user_message",
          message_id: "m-user-9",
          content: [
            {
              type: "markdown",
              text: "Read the project, then create utils.js with three small helper functions, write utils.test.js that tests them, and run it with node.",
            },
          ],
        },
      },
      {
        session_id: sessionId,
        seq: 2,
        ts_ms: ts + 200,
        payload: { kind: "turn_started", turn_id: "turn-9" },
      },
      // --- Iteration 1: explore the project. ---
      {
        session_id: sessionId,
        seq: 3,
        ts_ms: ts + 400,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoShellCall(sessionId, "ls -la", "call-9-ls"),
            turn_id: "turn-9",
          },
        },
      },
      {
        session_id: sessionId,
        seq: 4,
        ts_ms: ts + 500,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoExploreCall(sessionId),
            id: "call-9-glob",
            turn_id: "turn-9",
            status: { state: "completed" },
          },
        },
      },
      // Tool-result-only user_message feeding BOTH results back to the
      // model — pure plumbing, must NOT become a `user` row / turn boundary.
      {
        session_id: sessionId,
        seq: 5,
        ts_ms: ts + 600,
        payload: {
          kind: "user_message",
          message_id: "m-toolresult-9a",
          content: [
            {
              type: "tool_result",
              tool_use_id: "call-9-ls",
              content: [{ type: "markdown", text: "exit_code: 0\n\nstdout:\nutils.js\nutils.test.js\npackage.json" }],
              is_error: false,
            },
            {
              type: "tool_result",
              tool_use_id: "call-9-glob",
              content: [{ type: "markdown", text: "/preview/flex-app/utils.js\n/preview/flex-app/utils.test.js" }],
              is_error: false,
            },
          ],
        },
      },
      // --- Iteration 2: read the existing files (should cluster with the
      // Read call in iteration 3, below, since only tool-result plumbing —
      // now dropped — sits between them). ---
      {
        session_id: sessionId,
        seq: 6,
        ts_ms: ts + 800,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoReadCall(sessionId, "utils.js", "call-9-read-1", 1, 38),
            turn_id: "turn-9",
          },
        },
      },
      {
        session_id: sessionId,
        seq: 7,
        ts_ms: ts + 900,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoReadCall(sessionId, "utils.test.js", "call-9-read-2", 1, 37),
            turn_id: "turn-9",
          },
        },
      },
      {
        session_id: sessionId,
        seq: 8,
        ts_ms: ts + 1_000,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoReadCall(sessionId, "package.json", "call-9-read-3", 1, 33),
            turn_id: "turn-9",
          },
        },
      },
      // Another tool-result-only user_message — 3 more results fed back.
      {
        session_id: sessionId,
        seq: 9,
        ts_ms: ts + 1_100,
        payload: {
          kind: "user_message",
          message_id: "m-toolresult-9b",
          content: [
            {
              type: "tool_result",
              tool_use_id: "call-9-read-1",
              content: [{ type: "markdown", text: "Read utils.js" }],
              is_error: false,
            },
            {
              type: "tool_result",
              tool_use_id: "call-9-read-2",
              content: [{ type: "markdown", text: "Read utils.test.js" }],
              is_error: false,
            },
            {
              type: "tool_result",
              tool_use_id: "call-9-read-3",
              content: [{ type: "markdown", text: "Read package.json" }],
              is_error: false,
            },
          ],
        },
      },
      // --- Iteration 3: files already exist as required — narrate, then
      // just run the tests. ---
      {
        session_id: sessionId,
        seq: 10,
        ts_ms: ts + 1_300,
        payload: {
          kind: "assistant_message",
          message_id: "m-asst-9a",
          model: "deepseek/deepseek-v4-flash",
          content: [
            {
              type: "markdown",
              text: "Both files already exist with exactly what you described. Let me just run the tests.",
            },
          ],
        },
      },
      {
        session_id: sessionId,
        seq: 11,
        ts_ms: ts + 1_500,
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoShellCall(sessionId, "node utils.test.js", "call-9-node"),
            turn_id: "turn-9",
            result: {
              content: [{ type: "markdown", text: "exit_code: 0\n\nstdout:\n15/15 tests passed" }],
              is_error: false,
              structured: { exit_code: 0, success: true, truncated: false },
            },
          },
        },
      },
      // Final tool-result-only user_message — must ALSO be dropped, even
      // though it's the LAST user_message before the closing answer.
      {
        session_id: sessionId,
        seq: 12,
        ts_ms: ts + 1_600,
        payload: {
          kind: "user_message",
          message_id: "m-toolresult-9c",
          content: [
            {
              type: "tool_result",
              tool_use_id: "call-9-node",
              content: [{ type: "markdown", text: "exit_code: 0\n\nstdout:\n15/15 tests passed" }],
              is_error: false,
            },
          ],
        },
      },
      // Final genuine answer — the only row that renders outside the group.
      {
        session_id: sessionId,
        seq: 13,
        ts_ms: ts + 1_800,
        payload: {
          kind: "assistant_message",
          message_id: "m-asst-9b",
          model: "deepseek/deepseek-v4-flash",
          content: [
            {
              type: "markdown",
              text: "Already done — both files exist and all 15/15 tests pass. Nothing to create or change.",
            },
          ],
        },
      },
      {
        session_id: sessionId,
        seq: 14,
        ts_ms: ts + 1_900,
        payload: {
          kind: "turn_completed",
          turn_id: "turn-9",
          summary: {
            turn_id: "turn-9",
            stop_reason: "end_turn",
            usage: { input: 45_212, output: 493, reasoning: 84 },
            num_model_calls: 4,
            num_tool_calls: 6,
            duration_ms: 1_700,
          },
        },
      },
    ]
  }

  // Cancelled-turn coverage: turn without a completion event.
  if (sessionId === "preview-session-3") {
    return [
      {
        session_id: sessionId,
        seq: 1,
        ts_ms: ts,
        payload: {
          kind: "user_message",
          message_id: "m-user-3",
          content: [
            { type: "markdown", text: "Restructure the docs navigation tree." },
          ],
        },
      },
      {
        session_id: sessionId,
        seq: 2,
        ts_ms: ts + 1_000,
        payload: { kind: "turn_started", turn_id: "turn-3" },
      },
      {
        session_id: sessionId,
        seq: 3,
        ts_ms: ts + 3_000,
        payload: {
          kind: "tool_call_updated",
          call: { ...demoExploreCall(sessionId), turn_id: "turn-3" },
        },
      },
    ]
  }

  // Restart-during-AskUserQuestion coverage: replay ends on a dangling
  // `awaiting_permission` AskUserQuestion tool call with a matching
  // `question_requested` — no `turn_completed`/`session_error` ever follows,
  // mirroring an app restart that killed the turn mid-HITL-question. Verifies
  // the boot-time sweep closes this like any other dangling tool row (no
  // eternal "Working"), and that the client appends an "interrupted by
  // restart" meta row instead of leaving a stale, unresolvable question.
  if (sessionId === "preview-session-5") {
    return [
      {
        session_id: sessionId,
        seq: 1,
        ts_ms: ts,
        payload: {
          kind: "user_message",
          message_id: "m-user-5",
          content: [
            { type: "markdown", text: "Do on your own decision." },
          ],
        },
      },
      {
        session_id: sessionId,
        seq: 2,
        ts_ms: ts + 1_000,
        payload: { kind: "turn_started", turn_id: "turn-5" },
      },
      {
        session_id: sessionId,
        seq: 3,
        ts_ms: ts + 2_000,
        payload: {
          kind: "tool_call_updated",
          call: {
            id: "tool-ask-5",
            session_id: sessionId,
            turn_id: "turn-5",
            message_id: "m-asst-5",
            tool_name: "AskUserQuestion",
            input: {
              questions: [
                {
                  header: "Approach",
                  question: "Which layout should the composer use?",
                  options: [
                    { label: "Docked card", description: "Compact, above the composer" },
                    { label: "Modal", description: "Centered, blocks the chat" },
                  ],
                  multi_select: false,
                  allow_custom: true,
                },
              ],
            },
            read_only: true,
            origin: { origin: "model" },
            status: { state: "awaiting_permission", request_id: "question-5" },
            timing: { queued_at_ms: ts + 2_000, started_at_ms: ts + 2_000 },
          },
        },
      },
      {
        session_id: sessionId,
        seq: 4,
        ts_ms: ts + 2_050,
        payload: {
          kind: "question_requested",
          id: "question-5",
          questions: [
            {
              header: "Approach",
              question: "Which layout should the composer use?",
              options: [
                { label: "Docked card", description: "Compact, above the composer" },
                { label: "Modal", description: "Centered, blocks the chat" },
              ],
              multi_select: false,
              allow_custom: true,
            },
          ],
        },
      },
    ]
  }

  return [
    {
      session_id: sessionId,
      seq: 1,
      ts_ms: ts,
      payload: {
        kind: "user_message",
        message_id: "m-user-1",
        content: [
          {
            type: "markdown",
            text: "Tighten the chat shell spacing and hierarchy, then align it with the reference agents window design.",
          },
        ],
      },
    },
    // Plan snapshot before the turn (feeds the Plan tab + a timeline PlanCard).
    {
      session_id: sessionId,
      seq: 1.5,
      ts_ms: ts + 200,
      payload: {
        kind: "plan_updated",
        entries: [
          { content: "Audit current chat shell spacing", status: "completed" },
          { content: "Align composer rail with the timeline", status: "completed" },
          { content: "Rebuild the empty-state hero", status: "in_progress" },
          { content: "Verify both themes in preview", status: "pending" },
        ],
      },
    },
    {
      session_id: sessionId,
      seq: 2,
      ts_ms: ts + 500,
      payload: { kind: "turn_started", turn_id: "turn-1" },
    },
    {
      session_id: sessionId,
      seq: 3,
      ts_ms: ts + 3_000,
      payload: {
        kind: "tool_call_updated",
        call: demoExploreCall(sessionId),
      },
    },
    {
      session_id: sessionId,
      seq: 4,
      ts_ms: ts + 5_000,
      payload: {
        kind: "tool_call_updated",
        call: demoReadCall(
          sessionId,
          "packages/desktop/src/components/molecules/ToolCallChip.tsx",
          "tool-read-1",
          80,
          120,
        ),
      },
    },
    {
      session_id: sessionId,
      seq: 5,
      ts_ms: ts + 8_000,
      payload: {
        kind: "tool_call_updated",
        call: demoEditCall(sessionId, {
          id: "tool-edit-1",
          input: {
            file_path: "packages/desktop/src/components/organisms/TurnTimeline.tsx",
            old_string: "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9",
            new_string: "line1",
          },
        }),
      },
    },
    {
      session_id: sessionId,
      seq: 6,
      ts_ms: ts + 10_000,
      payload: {
        kind: "tool_call_updated",
        call: demoEditCall(sessionId, {
          id: "tool-edit-2",
          input: {
            file_path: "packages/desktop/src/components/molecules/ToolCallChip.tsx",
            old_string: "old",
            new_string: "n1\nn2\nn3\nn4\nn5\nn6\nn7\nn8\nn9\nn10\nn11\nn12\nn13\nn14\nn15\nn16\nn17\nn18\nn19\nn20\nn21\nn22\nn23\nn24\nn25\nn26\nn27\nn28\nn29\nn30\nn31\nn32\nn33\nn34\nn35\nn36\nn37\nn38\nn39\nn40\nn41\nn42\nn43\nn44\nn45\nn46\nn47\nn48\nn49\nn50\nn51\nn52\nn53\nn54\nn55\nn56\nn57\nn58\nn59\nn60\nn61\nn62\nn63\nn64\nn65\nn66\nn67\nn68\nn69\nn70\nn71\nn72\nn73\nn74\nn75\nn76\nn77\nn78\nn79\nn80\nn81\nn82\nn83\nn84\nn85\nn86\nn87\nn88\nn89\nn90\nn91\nn92\nn93\nn94\nn95\nn96\nn97\nn98\nn99\nn100\nn101\nn102\nn103\nn104\nn105\nn106\nn107\nn108\nn109\nn110\nn111\nn112\nn113\nn114\nn115\nn116\nn117\nn118\nn119\nn120\nn121\nn122\nn123\nn124\nn125\nn126\nn127\nn128\nn129\nn130\nn131\nn132\nn133\nn134\nn135\nn136\nn137\nn138\nn139\nn140\nn141\nn142\nn143\nn144\nn145\nn146\nn147\nn148\nn149\nn150\nn151\nn152\nn153\nn154\nn155\nn156\nn157\nn158\nn159\nn160\nn161\nn162\nn163\nn164\nn165\nn166\nn167\nn168\nn169\nn170\nn171\nn172\nn173\nn174",
          },
        }),
      },
    },
    {
      session_id: sessionId,
      seq: 7,
      ts_ms: ts + 12_000,
      payload: {
        kind: "tool_call_updated",
        call: demoEditCall(sessionId, {
          id: "tool-edit-3",
          input: {
            file_path: "packages/desktop/src/components/molecules/WorkGroup.tsx",
            old_string: "a",
            new_string: "a\nb\nc\nd\ne\nf",
          },
        }),
      },
    },
    {
      session_id: sessionId,
      seq: 8,
      ts_ms: ts + 14_000,
      payload: {
        kind: "tool_call_updated",
        call: demoShellCall(sessionId, "npx tsc --noEmit", "tool-shell-1"),
      },
    },
    {
      session_id: sessionId,
      seq: 9,
      ts_ms: ts + 16_000,
      payload: {
        kind: "tool_call_updated",
        call: demoShellCall(sessionId, "cargo check -p desktop", "tool-shell-2"),
      },
    },
    // Nested subagent block inside the turn's work rows.
    {
      session_id: sessionId,
      seq: 9.2,
      ts_ms: ts + 18_000,
      payload: {
        kind: "subagent_started",
        child_session: "preview-sub-1",
        task: "Explore the auth module",
        role: "explorer",
      },
    },
    {
      session_id: sessionId,
      seq: 9.4,
      ts_ms: ts + 20_000,
      payload: {
        kind: "subagent_event",
        child_session: "preview-sub-1",
        event: {
          kind: "tool_call_updated",
          call: {
            ...demoExploreCall("preview-sub-1"),
            id: "sub-tool-1",
            turn_id: "sub-turn-1",
            message_id: "sub-m-1",
            input: { pattern: "src/auth/**" },
          },
        },
      },
    },
    {
      session_id: sessionId,
      seq: 9.6,
      ts_ms: ts + 27_000,
      payload: {
        kind: "subagent_event",
        child_session: "preview-sub-1",
        event: {
          kind: "assistant_message",
          message_id: "sub-m-2",
          model: "anthropic/claude-sonnet-4",
          content: [
            {
              type: "markdown",
              text: "Auth module uses JWT sessions; refresh flow lives in `auth/refresh.ts`.",
            },
          ],
        },
      },
    },
    {
      session_id: sessionId,
      seq: 9.8,
      ts_ms: ts + 30_000,
      payload: {
        kind: "subagent_completed",
        child_session: "preview-sub-1",
        summary: {
          turn_id: "sub-turn-1",
          stop_reason: "end_turn",
          usage: { input: 9_400, output: 320 },
          num_model_calls: 1,
          num_tool_calls: 2,
          duration_ms: 12_000,
        },
      },
    },
    // Verifier verdict: an independent `Verify` call the goal loop ran to
    // grade the work above against a rubric (see EngineService::verify_goal_progress).
    // Renders as a distinct "verdict" row inside the work group's timeline
    // (useSessionEvents special-cases tool_name === "Verify") plus a small
    // glyph on the group's collapsed summary line.
    {
      session_id: sessionId,
      seq: 9.9,
      ts_ms: ts + 58_000,
      payload: {
        kind: "tool_call_updated",
        call: demoVerifyCall(sessionId),
      },
    },
    {
      session_id: sessionId,
      seq: 10,
      ts_ms: ts + 60_000,
      payload: {
        kind: "assistant_message",
        message_id: "m-asst-1",
        model: "anthropic/claude-sonnet-4",
        content: [
          {
            type: "thinking",
            text: "The composer rail and the timeline use different max widths — aligning both to the shared content rail fixes the vertical grid.",
          },
          {
            type: "markdown",
            text: [
              "Aligned the composer rail with the timeline and rebuilt the empty state.",
              "",
              "## What changed",
              "",
              "| Area | Change |",
              "| --- | --- |",
              "| Composer | Even `p-3` padding, shared rail |",
              "| Timeline | Bottom-aligned short threads |",
              "| Hero | Brand + title above composer |",
              "",
              "Everything sits on the same `--content-rail` now, so the header, transcript, and composer share one axis.",
            ].join("\n"),
          },
        ],
      },
    },
    {
      session_id: sessionId,
      seq: 11,
      ts_ms: ts + 69_500,
      payload: {
        kind: "turn_completed",
        turn_id: "turn-1",
        summary: {
          turn_id: "turn-1",
          stop_reason: "end_turn",
          usage: {
            input: 84_000,
            output: 2_400,
            cache_read: 71_000,
            cache_write: 12_800,
            reasoning: 640,
          },
          cost_usd: 0.0213,
          num_model_calls: 3,
          num_tool_calls: 7,
          duration_ms: 69_000,
        },
      },
    },
    {
      session_id: sessionId,
      seq: 11.5,
      ts_ms: ts + 70_000,
      payload: {
        kind: "compaction_boundary",
        summary: {
          summary_markdown: "Compacted earlier exploration into a short summary.",
          strategy: "summarize",
          tokens_before: 142_000,
          tokens_after: 8_200,
        },
      },
    },
    {
      session_id: sessionId,
      seq: 11.7,
      ts_ms: ts + 70_500,
      payload: {
        kind: "hook_fired",
        point: "PostToolUse",
        outcome: "continue",
      },
    },
  ]
}

export const isBrowserPreview = (): boolean =>
  typeof window !== "undefined" &&
  !(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__

export const browserInvoke = async <T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> => {
  switch (cmd) {
    case "hello":
      return { ok: true } as T
    case "user_identity":
      return { name: "Preview User" } as T
    case "write_temp_blob": {
      // No real filesystem in preview — `attachImageBlob` never actually
      // calls this command here (it short-circuits to an object URL), but a
      // fake path keeps this mock honest for any other future caller.
      const ext = String(args?.ext ?? "png").replace(/^\./, "")
      return `/preview/tmp/flex-paste-${Date.now()}.${ext}` as T
    }
    case "save_text_file": {
      // No real filesystem in preview — stash into sessionStorage under a
      // `<sessionId>::<relativePath>` key so a Save-then-reload round-trips
      // the same way the browserMock's other seeded stores do (see e.g. the
      // routines/MCP/memory mocks above), and return a fake absolute path
      // matching the shape the real command returns (`<cwd>/<relativePath>`).
      const sessionId = String(args?.sessionId ?? "")
      const relativePath = String(args?.relativePath ?? "")
      const content = String(args?.content ?? "")
      if (!relativePath.trim()) {
        throw new Error("path is required")
      }
      if (
        relativePath.startsWith("/") ||
        relativePath.split("/").some((seg) => seg === "..")
      ) {
        throw new Error(`path must not escape the session cwd: ${relativePath}`)
      }
      const session = sessions.find((s) => s.id === sessionId)
      const cwd = session?.cwd ?? "/preview/workspace"
      const key = `flex.saveTextFile.${sessionId}::${relativePath}`
      try {
        window.sessionStorage.setItem(key, content)
      } catch {
        // sessionStorage can throw (quota, privacy mode) — non-fatal for preview.
      }
      return `${cwd}/${relativePath}` as T
    }
    case "export_diagnostics_bundle": {
      const payload = String(args?.frontendPayload ?? "")
      const key = `flex.diagnostics.${Date.now()}`
      try {
        window.sessionStorage.setItem(key, payload)
      } catch {
        // ignore
      }
      return `/preview/logs/diagnostics-${Date.now()}.txt` as T
    }
    case "index_status": {
      const cwd = String(args?.cwd ?? "/preview/repo")
      return {
        repoRoot: cwd,
        indexDir: `/preview/app-data/agentloop/index/mock`,
        fileCount: mockPlugins.index ? 42 : 0,
        symbolCount: mockPlugins.index ? 128 : 0,
        embeddedChunkCount: 0,
        ready: mockPlugins.index,
      } as T
    }
    case "index_rebuild": {
      const cwd = String(args?.cwd ?? "/preview/repo")
      return {
        status: {
          repoRoot: cwd,
          indexDir: `/preview/app-data/agentloop/index/mock`,
          fileCount: 42,
          symbolCount: 128,
          embeddedChunkCount: 0,
          ready: true,
        },
        stats: { added: 2, changed: 1, removed: 0, unchanged: 39 },
      } as T
    }
    case "app_version":
      return "0.1.0-preview" as T
    case "git_is_repo": {
      const cwd = String(args?.cwd ?? "")
      return (cwd !== NON_GIT_DEMO_CWD) as T
    }
    case "git_branch": {
      const cwd = String(args?.cwd ?? "")
      return (mockBranchByCwd[cwd] ?? "main") as T
    }
    case "git_list_branches":
      return mockBranches as T
    case "git_status": {
      const cwd = String(args?.cwd ?? "")
      if (cwd === NON_GIT_DEMO_CWD) return mockStatusSummary([]) as T
      if (cwd === LARGE_CHANGES_DEMO_CWD) {
        return mockStatusSummary(mockLargeGitStatus) as T
      }
      return mockStatusSummary(mockGitStatus) as T
    }
    // Session-scoped: files changed since the session's baseline. Mocked as
    // a subset of `mockGitStatus` (drops the untracked file) so the preview
    // visibly demonstrates the scoping vs. the full-repo `git_status` mock —
    // except for the large-changes demo session, which returns the full
    // synthetic list unfiltered (mirrors a real session whose baseline is
    // empty because every file is brand-new).
    case "git_status_since_baseline": {
      const sessionId = String(args?.sessionId ?? "")
      const session = sessions.find((s) => s.id === sessionId)
      if (session?.cwd === NON_GIT_DEMO_CWD) return mockStatusSummary([]) as T
      if (session?.cwd === LARGE_CHANGES_DEMO_CWD) {
        return mockStatusSummary(mockLargeGitStatus) as T
      }
      return mockStatusSummary(mockGitStatus) as T
    }
    // Commit bar (non-isolated sessions): mock a successful commit that
    // clears the changed-files list (mirrors a real `git add -A && git
    // commit` against the mocked working tree) and a no-op push.
    case "git_commit":
      mockGitStatus = []
      return "abc1234" as T
    case "git_push":
      return undefined as T
    // Commit center (Changes tab, spec #48): selective staging — only drop
    // the paths the caller actually selected, so the preview visibly
    // demonstrates partial-commit behavior (unlike `git_commit`'s
    // stage-everything mock above).
    case "git_commit_paths": {
      const paths = (args?.paths as string[] | undefined) ?? []
      mockGitStatus = mockGitStatus.filter((f) => !paths.includes(f.path))
      return "abc1234" as T
    }
    case "git_commit_and_push": {
      const paths = (args?.paths as string[] | undefined) ?? []
      mockGitStatus = mockGitStatus.filter((f) => !paths.includes(f.path))
      return "abc1234" as T
    }
    case "git_create_branch_and_commit": {
      const branch = String(args?.branch ?? "preview-branch")
      const paths = (args?.paths as string[] | undefined) ?? []
      mockGitStatus = mockGitStatus.filter((f) => !paths.includes(f.path))
      const cwd = Object.keys(mockBranchByCwd)[0]
      if (cwd) mockBranchByCwd = { ...mockBranchByCwd, [cwd]: branch }
      return "abc1234" as T
    }
    case "git_create_pr": {
      const paths = (args?.paths as string[] | undefined) ?? []
      mockGitStatus = mockGitStatus.filter((f) => !paths.includes(f.path))
      return {
        commitSha: "abc1234",
        prUrl: "https://github.com/preview/flex-app/pull/42",
        degradedReason: null,
      } as T
    }
    case "suggest_commit_message":
      return "Update tokens and customize page" as T
    case "git_diff":
    case "review_file_diff": {
      const p = String(args?.path ?? "file")
      return mockDiffFor(p) as T
    }
    // Per-file / per-hunk review actions (Changes tab Keep/Undo). Mutates
    // `mockGitStatus` plausibly so the preview flow is demonstrable:
    // undo removes the file from the changed-files list (mirrors the real
    // command reverting it to its base state); keep no-ops on the list
    // (only the isolated session's base repo copy changes — the worktree,
    // which the Changes tab always reflects, is untouched).
    case "review_undo_file": {
      const p = String(args?.path ?? "")
      mockGitStatus = mockGitStatus.filter((f) => f.path !== p)
      return undefined as T
    }
    case "review_keep_file":
      return undefined as T
    case "review_apply_patch": {
      // Best-effort mock: a hunk-level Keep/Undo doesn't change whether the
      // file as a whole still differs from HEAD in this simplified mock (no
      // real hunk-splitting of `mockDiffFor`'s content), so this is a no-op
      // beyond acknowledging the call succeeded.
      return undefined as T
    }
    case "git_checkout": {
      const cwd = String(args?.cwd ?? "")
      const branch = String(args?.branch ?? "")
      if (!mockBranches.includes(branch)) {
        throw new Error(`Branch not found: ${branch}`)
      }
      mockBranchByCwd = { ...mockBranchByCwd, [cwd]: branch }
      return undefined as T
    }
    case "list_files": {
      const query = String(args?.query ?? "").trim().toLowerCase()
      const scored = mockFiles
        .map((path) => {
          const name = path.split("/").pop() ?? path
          const nameL = name.toLowerCase()
          const pathL = path.toLowerCase()
          let score: number | null = null
          if (!query) score = 100
          else if (nameL.startsWith(query)) score = 0
          else if (nameL.includes(query)) score = 1
          else if (pathL.includes(query)) score = 2
          return score === null ? null : { score, path, name }
        })
        .filter((x): x is { score: number; path: string; name: string } => !!x)
      scored.sort(
        (a, b) => a.score - b.score || a.path.length - b.path.length,
      )
      return scored
        .slice(0, 50)
        .map(({ path, name }) => ({ path, name })) as T
    }
    case "is_configured": {
      if (
        typeof window !== "undefined" &&
        new URLSearchParams(window.location.search).get("welcome") === "1"
      ) {
        return false as T
      }
      return configured as T
    }
    case "get_provider_config":
      return providerConfig() as T
    case "list_builtin_providers":
      return [
        { id: "anthropic", label: "Anthropic", requiresApiKey: true },
        { id: "openai", label: "OpenAI", requiresApiKey: true },
        { id: "ollama", label: "Ollama", requiresApiKey: false },
        { id: "copilot", label: "GitHub Copilot", requiresApiKey: false },
      ] as BuiltinProvider[] as T
    case "copilot_auth_status":
      return { signedIn: false } as T
    case "copilot_auth_start":
      return {
        sessionId: "mock-copilot-auth",
        userCode: "ABCD-1234",
        verificationUri: "https://github.com/login/device",
        expiresIn: 900,
      } as T
    case "copilot_auth_wait":
      return { signedIn: true } as T
    case "copilot_auth_cancel":
      return undefined as T
    case "list_models":
      return models as T
    case "list_providers":
      return ["anthropic", "openai"] as T
    case "validate_provider":
      return models as T
    case "save_provider_config": {
      configured = true
      const input = args?.input as
        | {
            plugins?: {
              search: boolean
              index: boolean
              autoContext?: boolean
              learning: boolean
              verifier: boolean
            }
          }
        | undefined
      if (input?.plugins) {
        mockPlugins = {
          search: input.plugins.search,
          index: input.plugins.index,
          autoContext: input.plugins.autoContext ?? false,
          learning: input.plugins.learning,
          verifier: input.plugins.verifier,
        }
        try {
          window.sessionStorage.setItem(
            "mock-plugins",
            JSON.stringify(mockPlugins),
          )
        } catch {
          // Non-fatal in preview.
        }
      }
      return providerConfig() as T
    }
    // No-op mock for the Security section's storage-mode switch — native
    // Keychain migration can't be exercised in browser preview, so this
    // just round-trips the choice (see `secrets::SecretsStore::switch_mode`
    // for the real migration this stands in for).
    case "set_secret_storage": {
      const mode = String(args?.mode ?? "file")
      if (mode === "file" || mode === "keychain") {
        mockSecretStorage = mode as SecretStorageMode
        try {
          window.sessionStorage.setItem("mock-secret-storage", mockSecretStorage)
        } catch {
          // Non-fatal in preview.
        }
      }
      return providerConfig() as T
    }
    case "profiles_list":
      return mockProfiles.map(mockProfileView) as T
    case "profile_upsert": {
      const input = (args?.profile ?? {}) as ProviderProfileInput
      const id = input.id?.trim() || `profile-${++mockProfileSeq}`
      const existing = mockProfiles.find((p) => p.id === id)
      const hasKey = input.apiKey?.trim() ? true : (existing?.hasKey ?? false)
      const updated: MockProfile = {
        id,
        label: input.label,
        provider: input.provider,
        baseUrl: input.baseUrl,
        region: input.region,
        defaultModel: input.defaultModel,
        fallbackModels: input.fallbackModels,
        defaultIsolation: input.defaultIsolation as string | undefined,
        hasKey,
      }
      mockProfiles = [...mockProfiles.filter((p) => p.id !== id), updated]
      if (!mockActiveProfileId) {
        mockActiveProfileId = id
        saveMockActiveProfileId()
      }
      saveMockProfiles()
      return mockProfileView(updated) as T
    }
    case "profile_remove": {
      const id = String(args?.id ?? "")
      if (id === mockActiveProfileId) {
        throw new Error(
          "cannot remove the active connection — activate another one first",
        )
      }
      mockProfiles = mockProfiles.filter((p) => p.id !== id)
      saveMockProfiles()
      return undefined as T
    }
    case "profile_activate": {
      const id = String(args?.id ?? "")
      if (!mockProfiles.some((p) => p.id === id)) {
        throw new Error(`connection not found: ${id}`)
      }
      mockActiveProfileId = id
      saveMockActiveProfileId()
      const active = mockProfiles.find((p) => p.id === id)
      return {
        ...providerConfig(),
        preferredProvider: active?.provider,
        baseUrl: active?.baseUrl,
        region: active?.region,
        defaultModel: active?.defaultModel,
        defaultIsolation: active?.defaultIsolation ?? "never",
      } as T
    }
    case "validate_profile":
      return models as T
    case "list_commands":
      return [
        {
          name: "plan",
          description: "Design before coding",
          argsHint: "task",
        },
        {
          name: "review",
          description: "Review recent changes",
        },
        {
          name: "explain",
          description: "Explain code or a concept",
          argsHint: "topic",
        },
        {
          name: "fix",
          description: "Propose a focused fix",
          argsHint: "bug",
        },
      ] as T
    case "is_isolated": {
      const id = String(args?.sessionId ?? "")
      const found = sessions.find((s) => s.id === id)
      // Fall back to the original hardcoded demo session so existing preview
      // scenarios (e.g. IsolationBadge stories) keep working even though it
      // predates `base_cwd` being set on demoSession.
      return (!!found?.base_cwd || id === "preview-session-1") as T
    }
    case "workspace_status": {
      const id = String(args?.sessionId ?? "")
      const found = sessions.find((s) => s.id === id)
      const isIsolatedSession = !!found?.base_cwd || id === "preview-session-1"
      return (isIsolatedSession
        ? { filesChanged: 3, summary: "+82 -15" }
        : null) as T
    }
    case "integrate_session":
      return { status: "empty" } as T
    case "discard_session":
    case "revert":
      return undefined as T
    case "list_sessions":
      return sessions as T
    case "create_session": {
      const input = (args?.input ?? {}) as CreateSessionInput
      const isIsolatedRequest = input.isolation === "required"
      const cwd = input.cwd ?? "/Users/preview/project"
      const session = demoSession({
        id: `preview-session-${sessions.length + 1}`,
        title: input.title ?? "New Agent",
        cwd,
        model: input.model ?? "anthropic/claude-sonnet-4",
        isolation: input.isolation,
        // Mirrors the real engine: an isolated session gets a private
        // worktree cwd, with `base_cwd` pointing back at the origin repo —
        // that's what `is_isolated`/`IsolationBadge` key off of.
        ...(isIsolatedRequest
          ? { base_cwd: cwd, cwd: `${cwd}/.flex-worktrees/session-${sessions.length + 1}` }
          : {}),
        created_at_ms: now(),
        updated_at_ms: now(),
      })
      sessions = [session, ...sessions]
      if (session.cwd && !mockBranchByCwd[session.cwd]) {
        mockBranchByCwd = { ...mockBranchByCwd, [session.cwd]: "main" }
      }
      return session as T
    }
    case "session_meta": {
      const id = String(args?.sessionId ?? "")
      const found = sessions.find((s) => s.id === id)
      if (!found) throw new Error(`Session not found: ${id}`)
      return found as T
    }
    case "resume_session":
      return undefined as T
    case "update_session": {
      const id = String(args?.sessionId ?? "")
      const patch = (args?.patch ?? {}) as UpdateSessionInput
      sessions = sessions.map((s) =>
        s.id === id
          ? {
              ...s,
              ...(patch.title !== undefined ? { title: patch.title } : {}),
              ...(patch.model !== undefined ? { model: patch.model } : {}),
              ...(patch.cwd !== undefined ? { cwd: patch.cwd } : {}),
              updated_at_ms: now(),
            }
          : s,
      )
      const updated = sessions.find((s) => s.id === id)
      if (!updated) throw new Error(`Session not found: ${id}`)
      if (patch.cwd && !mockBranchByCwd[patch.cwd]) {
        mockBranchByCwd = { ...mockBranchByCwd, [patch.cwd]: "main" }
      }
      return updated as T
    }
    case "delete_session": {
      const id = String(args?.sessionId ?? "")
      sessions = sessions.filter((s) => s.id !== id)
      return undefined as T
    }
    case "suggest_session_title": {
      // Mock stand-in for the real one-shot LLM title completion — just
      // enough to exercise the fire-once auto-title UX in preview.
      const promptText = String(args?.promptText ?? "")
      const words = promptText
        .replace(/[`*_#>]/g, " ")
        .trim()
        .split(/\s+/)
        .filter(Boolean)
        .slice(0, 4)
      if (words.length === 0) throw new Error("empty title generated")
      const title = words
        .map((w) => w[0]!.toUpperCase() + w.slice(1))
        .join(" ")
      return title as T
    }
    case "replay":
      return timelineEvents(String(args?.sessionId ?? "")) as T
    case "subscribe_session":
    case "unsubscribe_session":
      return undefined as T
    case "cancel": {
      const sessionId = String(args?.sessionId ?? "")
      const pending = pendingTurns.get(sessionId)
      if (!pending) return undefined as T
      // Stop the mock's own streaming timers right away (matches the real
      // desktop's Composer.handleStop clearing local UI state instantly),
      // but — unlike the old mock — do NOT drop `pendingTurns`/emit
      // `turn_completed` synchronously. The real engine's `turn_gate` isn't
      // released until the cancelled turn's task actually unwinds
      // (MOCK_CANCEL_TEARDOWN_MS below), and a `prompt()` that races into
      // that window must still see "a turn is already in progress" so the
      // stop -> resend queue-and-drain path is exercised for real.
      for (const id of pending.timers) window.clearTimeout(id)
      pending.timers.length = 0
      const teardown = window.setTimeout(() => {
        // Cancel may have been superseded (e.g. the turn already finished
        // naturally, or a second cancel fired) — only tear down if this
        // exact pending entry is still the live one.
        if (pendingTurns.get(sessionId) !== pending) return
        pendingTurns.delete(sessionId)
        emit({
          session_id: sessionId,
          seq: 199,
          ts_ms: now(),
          payload: {
            kind: "turn_completed",
            turn_id: pending.turnId,
            summary: {
              turn_id: pending.turnId,
              stop_reason: "cancelled",
              usage: { input: 1_200, output: 40 },
              num_model_calls: 1,
              num_tool_calls: 0,
              duration_ms: Math.max(now() - pending.startedAt, 50),
            },
          },
        })
      }, MOCK_CANCEL_TEARDOWN_MS)
      pending.timers.push(teardown)
      return undefined as T
    }
    case "background_list": {
      // Mirrors the demo `bgCall` seeded in the timeline fixture below
      // (`tool-bg-<ts>`, `npm run dev`) — good enough for the "background
      // processes" panel to have something to render in the preview. Real
      // data comes from `EngineService::background_list` in production.
      const sessionId = String(args?.sessionId ?? "")
      const killed = mockKilledBackgroundIds.has(sessionId)
      const entries: BackgroundProcessDto[] = []
      const bgId = mockBackgroundIdBySession.get(sessionId)
      if (bgId) {
        entries.push({
          process_id: bgId,
          command: "npm run dev",
          running: !killed,
          started_at_ms: now() - 59_000,
          exit_code: killed ? 0 : null,
        })
      }
      return entries as T
    }
    case "background_kill": {
      const sessionId = String(args?.sessionId ?? "")
      const processId = String(args?.processId ?? "")
      mockKilledBackgroundIds.add(sessionId)
      // Flip the mocked background call's `structured.running` to false and
      // append the engine's exit-marker chunk, so a manual Stop click in the
      // preview reaches the same "exited" state the marker alone produces.
      emit({
        session_id: sessionId,
        seq: 102.7,
        ts_ms: now(),
        payload: {
          kind: "exec_chunk",
          call_id: processId,
          stream: "stdout",
          text: "[process exited with code 0]\n",
        },
      })
      return undefined as T
    }
    case "background_demote": {
      // Mock for `MOVE-TO-BACKGROUND`: flips the demo running "cargo build"
      // shell call (see `shellCallId`/`shellCall` in the timeline fixture
      // below) into the same background presentation `isDemotedBashCall`
      // detects for a real engine's demote result — same structured
      // `{"process_id", "running"}` shape a `run_in_background` start uses,
      // per the unified detection in `ToolStepGroup`.
      const sessionId = String(args?.sessionId ?? "")
      const callId = String(args?.callId ?? "")
      if (!callId || mockDemotedCallIds.has(callId)) {
        return false as T
      }
      mockDemotedCallIds.add(callId)
      emit({
        session_id: sessionId,
        seq: 102.35,
        ts_ms: now(),
        payload: {
          kind: "tool_call_updated",
          call: {
            ...demoShellCall(sessionId, "cargo build", callId),
            status: { state: "completed" },
            result: {
              content: [
                {
                  type: "markdown",
                  text: `Moved to background (process ${callId}). Output so far:\n$ cargo build\n   Compiling desktop v0.1.0\n\n[output continues in the agent terminal; use Bash background_action status/kill with process_id ${callId}]`,
                },
              ],
              is_error: false,
              structured: { process_id: callId, pid: 5150, running: true, truncated: false },
            },
          },
        },
      })
      return true as T
    }
    case "respond_permission": {
      const input = (args?.input ?? args ?? {}) as {
        sessionId: string
        requestId: string
        decision: string
      }
      emit({
        session_id: input.sessionId,
        seq: 198,
        ts_ms: now(),
        payload: {
          kind: "permission_resolved",
          id: input.requestId,
          decision: input.decision,
        },
      })
      return undefined as T
    }
    case "respond_question": {
      const input = (args?.input ?? args ?? {}) as {
        sessionId: string
        requestId: string
        answers?: { question: string; selected: string[] }[]
      }
      emit({
        session_id: input.sessionId,
        seq: 197,
        ts_ms: now(),
        payload: {
          kind: "question_resolved",
          id: input.requestId,
          answers: input.answers ?? [],
        },
      })
      return undefined as T
    }
    case "prompt": {
      const input = (args?.input ?? args ?? {}) as PromptCommandInput
      const sessionId = input.sessionId
      // Mirror the real engine's `AgentError::TurnInProgress` rejection (see
      // `agentloop_core::agent`) — a second `prompt` for a session that
      // already has one in flight is rejected, not silently restarted. Lets
      // the desktop layer's queue-on-rejection recovery path (Composer's
      // handleSend) be exercised in preview/mock mode too.
      if (pendingTurns.has(sessionId)) {
        throw new Error(`a turn is already in progress for session ${sessionId}`)
      }
      // Observable hook for preview verification — every prompt logs the
      // resolved `permissionMode` so Settings → Behavior → Permissions
      // (`defaultPermissionMode` → `modeToPermission`) can be sanity-checked
      // without instrumenting the store directly.
      console.debug("[mock] prompt: permissionMode resolved", {
        composerMode: input.composerMode,
        permissionMode: input.permissionMode,
      })
      const ts = now()
      const turnId = `turn-${ts}`
      const messageId = `m-asst-${ts}`
      const isPlan = input.permissionMode === "plan"
      if (input.composerMode === "flex") {
        // Observable hook for preview verification — the Flex mode plumbs an
        // explicit `composerMode` flag through `prompt()` alongside its
        // derived `permissionMode: "dont_ask"` (see `ModePicker.tsx`'s
        // `modeToPermission` and `commands.rs::prompt`'s orchestrator system
        // prompt). No mock behavior otherwise changes for this mode.
        console.debug("[mock] prompt: composerMode=flex, permissionMode=dont_ask", {
          sessionId,
          permissionMode: input.permissionMode,
        })
      }
      // Mirror engine BypassPermissions: Settings "Bypass all" /
      // Composer Shield resolve to `permissionMode: "bypass_permissions"`
      // and skip tool PermissionPrompt. AskUserQuestion
      // (`wantsQuestion` → `question_requested`) still fires by design.
      const bypassPermissions = input.permissionMode === "bypass_permissions"
      const wantsPermission =
        !bypassPermissions &&
        /\b(permission|allow|approve)\b/i.test(input.text)
      const wantsQuestion = /\bquestion\b/i.test(input.text)
      const wantsWorkflow = /\bworkflow\b/i.test(input.text)
      const wantsVerify = /\bverif(y|ied|ication)\b/i.test(input.text)
      clearPendingTurn(sessionId)

      emit({
        session_id: sessionId,
        seq: 100,
        ts_ms: ts,
        payload: {
          kind: "user_message",
          message_id: `m-user-${ts}`,
          content: [{ type: "markdown", text: input.text }],
        },
      })

      emit({
        session_id: sessionId,
        seq: 101,
        ts_ms: ts + 1,
        payload: { kind: "turn_started", turn_id: turnId },
      })

      const liveCall: ToolCall = {
        ...demoExploreCall(sessionId),
        id: `tool-${ts}`,
        turn_id: turnId,
        message_id: messageId,
        status: { state: "running" },
        result: undefined,
      }

      const timers: number[] = []

      // RunWorkflow demo: a 3-step pipeline (2 sequential tasks, then a
      // parallel fan-out of 2) staggered over ~6s so the preview shows the
      // WorkflowGroup block progress step-by-step, exactly like a real
      // engine run would stream it (tool_call_updated for the call itself,
      // subagent_started/subagent_event/subagent_completed per step — see
      // packages/engine/crates/loop/src/workflow.rs).
      if (wantsWorkflow) {
        const workflowCallId = `tool-workflow-${ts}`
        const workflowInput = {
          steps: [
            {
              kind: "task",
              role: "explorer",
              prompt: "Map how the timeline renders subagent events.",
              label: "map event flow",
            },
            {
              kind: "task",
              role: "explorer",
              prompt: "Find the WorkGroup/ToolStepGroup styling conventions.",
              label: "survey styling conventions",
            },
            {
              kind: "parallel",
              tasks: [
                {
                  role: "worker",
                  prompt: "Draft the WorkflowGroup component.",
                  label: "draft WorkflowGroup",
                },
                {
                  role: "worker",
                  prompt: "Wire workflow rows into useSessionEvents.",
                  label: "wire timeline hook",
                },
              ],
            },
          ],
        }
        const workflowCall = (
          status: ToolCall["status"],
          result?: ToolCall["result"],
        ): ToolCall => ({
          id: workflowCallId,
          session_id: sessionId,
          turn_id: turnId,
          message_id: messageId,
          tool_name: "RunWorkflow",
          input: workflowInput,
          read_only: true,
          origin: { origin: "model" },
          status,
          timing: { queued_at_ms: now() },
          result,
        })

        timers.push(
          window.setTimeout(() => {
            emit({
              session_id: sessionId,
              seq: 102.05,
              ts_ms: now(),
              payload: { kind: "tool_call_updated", call: workflowCall({ state: "running" }) },
            })
          }, 200),
        )

        const subagentStep = (
          seqBase: number,
          delayMs: number,
          childSession: string,
          task: string,
          role: string,
          replyText: string,
        ) => {
          timers.push(
            window.setTimeout(() => {
              emit({
                session_id: sessionId,
                seq: seqBase,
                ts_ms: now(),
                payload: {
                  kind: "subagent_started",
                  child_session: childSession,
                  task,
                  role,
                  call_id: workflowCallId,
                },
              })
            }, delayMs),
          )
          timers.push(
            window.setTimeout(() => {
              emit({
                session_id: sessionId,
                seq: seqBase + 0.3,
                ts_ms: now(),
                payload: {
                  kind: "subagent_event",
                  child_session: childSession,
                  event: {
                    kind: "assistant_message",
                    message_id: `${childSession}-m`,
                    model: "anthropic/claude-sonnet-4",
                    content: [{ type: "markdown", text: replyText }],
                  },
                },
              })
            }, delayMs + 700),
          )
          timers.push(
            window.setTimeout(() => {
              emit({
                session_id: sessionId,
                seq: seqBase + 0.6,
                ts_ms: now(),
                payload: {
                  kind: "subagent_completed",
                  child_session: childSession,
                  summary: {
                    turn_id: `${childSession}-turn`,
                    stop_reason: "end_turn",
                    usage: { input: 6_200, output: 240 },
                    num_model_calls: 1,
                    num_tool_calls: 1,
                    duration_ms: 1_100,
                  },
                },
              })
            }, delayMs + 1_100),
          )
        }

        // Step 1 (task) — starts ~0.5s in, completes ~1.6s in.
        subagentStep(
          102.1,
          500,
          "preview-wf-1",
          "map event flow",
          "explorer",
          "Nested subagent events relay through `subagent_event`, matched to their parent by `child_session`.",
        )
        // Step 2 (task) — starts after step 1 completes, ~1.8s in.
        subagentStep(
          102.2,
          1_800,
          "preview-wf-2",
          "survey styling conventions",
          "explorer",
          "WorkGroup/ToolStepGroup use 13px rows, Collapsible, and padding-only indents — no border rails.",
        )
        // Step 3 (parallel) — both tasks start together after step 2, ~3.1s in.
        subagentStep(
          102.3,
          3_100,
          "preview-wf-3a",
          "draft WorkflowGroup",
          "worker",
          "Drafted the collapsible step list with inferred status icons.",
        )
        subagentStep(
          102.31,
          3_100,
          "preview-wf-3b",
          "wire timeline hook",
          "worker",
          "Routed subagent_started/_event/_completed into the workflow row when call_id matches.",
        )

        timers.push(
          window.setTimeout(() => {
            emit({
              session_id: sessionId,
              seq: 102.9,
              ts_ms: now(),
              payload: {
                kind: "tool_call_updated",
                call: workflowCall(
                  { state: "completed" },
                  {
                    content: [
                      { type: "markdown", text: "Workflow finished: 3/3 steps." },
                    ],
                    is_error: false,
                  },
                ),
              },
            })
          }, 4_400),
        )
      }

      // Verify demo: emits a running `Verify` call, then settles it to a
      // completed verdict ~1.8s later — same wire shape as a real
      // `EngineService::verify_goal_progress` run (tool_call_updated with
      // tool_name "Verify", verdict in result.structured). Shows the
      // "Verifying…" state (VerdictBadge) transition to the pass/fail badge,
      // both in the timeline's verdict row and the Plan tab's Verification
      // section.
      if (wantsVerify) {
        const verifyCallId = `tool-verify-${ts}`
        const wantsFail = /\bfail\b/i.test(input.text)
        timers.push(
          window.setTimeout(() => {
            emit({
              session_id: sessionId,
              seq: 102.5,
              ts_ms: now(),
              payload: {
                kind: "tool_call_updated",
                call: {
                  ...demoVerifyCall(sessionId, {
                    id: verifyCallId,
                    turn_id: turnId,
                    message_id: messageId,
                    status: { state: "running" },
                    result: undefined,
                  }),
                },
              },
            })
          }, 400),
        )
        timers.push(
          window.setTimeout(() => {
            emit({
              session_id: sessionId,
              seq: 102.6,
              ts_ms: now(),
              payload: {
                kind: "tool_call_updated",
                call: demoVerifyCall(sessionId, {
                  id: verifyCallId,
                  turn_id: turnId,
                  message_id: messageId,
                  status: { state: "completed" },
                  result: wantsFail
                    ? {
                        content: [
                          {
                            type: "markdown",
                            text: "Verdict: fail — rubric not met.",
                          },
                        ],
                        is_error: false,
                        structured: {
                          outcome: "fail",
                          findings: [
                            "tsc --noEmit still reports 2 errors in TurnTimeline.tsx.",
                            "WorkGroup summary line doesn't show the verdict glyph yet.",
                          ],
                          confidence: 0.81,
                        },
                      }
                    : undefined,
                }),
              },
            })
          }, 2_200),
        )
      }

      if (isPlan) {
        timers.push(
          window.setTimeout(() => {
            emit({
              session_id: sessionId,
              seq: 101.2,
              ts_ms: now(),
              payload: {
                kind: "plan_updated",
                entries: [
                  { content: "Analyze the request", status: "completed" },
                  { content: "Draft an approach", status: "in_progress" },
                  {
                    content: "List risks and open questions",
                    status: "pending",
                  },
                ],
              },
            })
          }, 80),
        )

        timers.push(
          window.setTimeout(() => {
            emit({
              session_id: sessionId,
              seq: 103.5,
              ts_ms: now(),
              payload: {
                kind: "tool_call_updated",
                call: {
                  id: `plan-${ts}`,
                  session_id: sessionId,
                  turn_id: turnId,
                  message_id: messageId,
                  tool_name: "ExitPlanMode",
                  input: {
                    plan: "## Implementation plan\n\n1. Extract shared tokens\n2. Restyle the composer\n3. Verify both themes\n\n**Risk:** none — read-only pass.",
                  },
                  read_only: true,
                  origin: { origin: "model" },
                  status: { state: "completed" },
                  timing: { queued_at_ms: now() },
                },
              },
            })
          }, 1_600),
        )
      }

      if (wantsPermission) {
        timers.push(
          window.setTimeout(() => {
            emit({
              session_id: sessionId,
              seq: 101,
              ts_ms: now(),
              payload: {
                kind: "permission_requested",
                id: `perm-${ts}`,
                title: "Allow Bash?",
                detail: "npx tsc --noEmit",
                options: ["allow_once", "allow_always", "deny"],
                call_id: `tool-${ts}`,
              },
            })
          }, 200),
        )
      }

      if (wantsQuestion) {
        // Scripted `AskUserQuestion` / `question_requested` trigger — lets
        // preview verification exercise QuestionPrompt's step-by-step wizard
        // the same way `wantsPermission` exercises PermissionPrompt's.
        // Three mixed single/multi questions so the wizard's auto-advance,
        // Next/Back, and final Submit are all reachable in preview.
        timers.push(
          window.setTimeout(() => {
            emit({
              session_id: sessionId,
              seq: 101.5,
              ts_ms: now(),
              payload: {
                kind: "question_requested",
                id: `question-${ts}`,
                questions: [
                  {
                    header: "Approach",
                    question: "Which layout should the composer use?",
                    options: [
                      { label: "Docked card", description: "Compact, above the composer" },
                      { label: "Modal", description: "Centered, blocks the chat" },
                    ],
                    multi_select: false,
                    allow_custom: true,
                  },
                  {
                    header: "Coverage",
                    question: "Which platforms should this ship to first?",
                    options: [
                      { label: "macOS", description: "Primary dev target" },
                      { label: "Windows", description: "Largest install base" },
                      { label: "Linux", description: "Power users" },
                    ],
                    multi_select: true,
                    allow_custom: true,
                  },
                  {
                    header: "Rollout",
                    question: "How should this feature be released?",
                    options: [
                      { label: "Behind a flag", description: "Gradual, reversible" },
                      { label: "Direct to all users", description: "Fast, higher risk" },
                    ],
                    multi_select: false,
                    allow_custom: true,
                  },
                ],
              },
            })
          }, 200),
        )
      }

      timers.push(
        window.setTimeout(() => {
          emit({
            session_id: sessionId,
            seq: 102,
            ts_ms: now(),
            payload: { kind: "tool_call_updated", call: liveCall },
          })
        }, 300),
      )

      // Partial args + live progress notes while the tool runs.
      timers.push(
        window.setTimeout(() => {
          emit({
            session_id: sessionId,
            seq: 102.3,
            ts_ms: now(),
            payload: {
              kind: "tool_args_delta",
              call_id: liveCall.id,
              json_fragment: '{"pattern":"**/*.tsx"',
            },
          })
        }, 450),
      )
      timers.push(
        window.setTimeout(() => {
          emit({
            session_id: sessionId,
            seq: 102.6,
            ts_ms: now(),
            payload: {
              kind: "tool_progress",
              call_id: liveCall.id,
              note: "scanning src/…",
            },
          })
        }, 650),
      )
      timers.push(
        window.setTimeout(() => {
          emit({
            session_id: sessionId,
            seq: 102.8,
            ts_ms: now(),
            payload: {
              kind: "tool_progress",
              call_id: liveCall.id,
              note: "matched 8 files",
            },
          })
        }, 1_050),
      )

      timers.push(
        window.setTimeout(() => {
          emit({
            session_id: sessionId,
            seq: 103,
            ts_ms: now(),
            payload: {
              kind: "tool_call_updated",
              call: {
                ...liveCall,
                status: { state: "completed" },
                result: {
                  content: [
                    {
                      type: "markdown",
                      text: "Found 8 files matching pattern.",
                    },
                  ],
                  is_error: false,
                },
              },
            },
          })
        }, 1_400),
      )

      // Agent terminal demo — simulate a Bash tool call streaming `exec_chunk`
      // events, so the preview shows the read-only agent terminal live (the reference design
      // parity: routing lives in useGlobalSessionEvents, this only emits the
      // wire events a real engine run would produce).
      const shellCallId = `tool-shell-${ts}`
      const shellCall: ToolCall = {
        ...demoShellCall(sessionId, "cargo build", shellCallId),
        status: { state: "running" },
        result: undefined,
      }
      timers.push(
        window.setTimeout(() => {
          emit({
            session_id: sessionId,
            seq: 102.15,
            ts_ms: now(),
            payload: { kind: "tool_call_updated", call: shellCall },
          })
        }, 1_500),
      )
      const execLines: { stream: "stdout" | "stderr"; text: string }[] = [
        { stream: "stdout", text: "$ cargo build\n" },
        { stream: "stdout", text: "   Compiling desktop v0.1.0\n" },
        { stream: "stdout", text: "   Compiling engine v0.1.0\n" },
        { stream: "stderr", text: "warning: unused import: `foo`\n" },
        // Real error line — verifies the "N errors in output" badge / "Ask
        // Agent to fix" action on this (foreground, completed) shell row
        // (see execErrorScan.ts).
        {
          stream: "stderr",
          text: "error[E0425]: cannot find function `foo` in this scope\n",
        },
        { stream: "stdout", text: "    Finished dev [unoptimized] target(s)\n" },
        { stream: "stdout", text: "  ➜  Local:   http://localhost:5173/\n" },
      ]
      execLines.forEach((line, i) => {
        timers.push(
          window.setTimeout(() => {
            emit({
              session_id: sessionId,
              seq: 102.2 + i * 0.01,
              ts_ms: now(),
              payload: {
                kind: "exec_chunk",
                call_id: shellCallId,
                stream: line.stream,
                text: line.text,
              },
            })
          }, 1_650 + i * 150),
        )
      })
      timers.push(
        window.setTimeout(
          () => {
            // Skip if `background_demote` already flipped this call — don't
            // stomp the "Moved to background" result with a plain
            // "completed" one.
            if (mockDemotedCallIds.has(shellCallId)) return
            emit({
              session_id: sessionId,
              seq: 102.3,
              ts_ms: now(),
              payload: {
                kind: "tool_call_updated",
                call: {
                  ...shellCall,
                  status: { state: "completed" },
                  result: {
                    content: [{ type: "markdown", text: "exit_code: 0" }],
                    is_error: false,
                    structured: { exit_code: 0, success: true, truncated: false },
                  },
                },
              },
            })
          },
          1_650 + execLines.length * 150,
        ),
      )

      // Background-process demo — a `Bash` call started with
      // `run_in_background: true`, rendering as a distinct row in
      // ToolStepGroup (see `isBackgroundBashCall`/`BackgroundRow`). Emits the
      // start call's `tool_call_updated` with `structured.running: true`,
      // streams a couple of `exec_chunk`s, then appends the engine's
      // `[process exited with code N]` marker chunk — the row should flip to
      // "exited" from that marker alone, independent of any further
      // `tool_call_updated`. `background_kill` (see the invoke case below)
      // flips `bgRunning` so a manual Stop click also reaches an exited
      // state without waiting for the marker.
      const bgCallId = `tool-bg-${ts}`
      mockBackgroundIdBySession.set(sessionId, bgCallId)
      const bgCall: ToolCall = {
        id: bgCallId,
        session_id: sessionId,
        turn_id: "turn-1",
        message_id: "m-asst-1",
        tool_name: "Bash",
        input: { command: "npm run dev", run_in_background: true },
        read_only: false,
        origin: { origin: "model" },
        status: { state: "completed" },
        timing: {
          queued_at_ms: now() - 60_000,
          started_at_ms: now() - 59_000,
          finished_at_ms: now() - 59_000,
        },
        result: {
          content: [
            {
              type: "markdown",
              text: "Started background process tool-bg-1 (pid 4242), now running.",
            },
          ],
          is_error: false,
          structured: { process_id: bgCallId, pid: 4242, running: true, truncated: false },
        },
      }
      timers.push(
        window.setTimeout(() => {
          emit({
            session_id: sessionId,
            seq: 102.4,
            ts_ms: now(),
            payload: { kind: "tool_call_updated", call: bgCall },
          })
        }, 1_700),
      )
      const bgLines: { stream: "stdout" | "stderr"; text: string }[] = [
        { stream: "stdout", text: "> flex-desktop@0.1.0 dev\n" },
        { stream: "stdout", text: "> vite\n" },
        { stream: "stdout", text: "  VITE ready in 312 ms\n" },
        { stream: "stdout", text: "  ➜  Local:   http://localhost:1421/\n" },
      ]
      bgLines.forEach((line, i) => {
        timers.push(
          window.setTimeout(() => {
            emit({
              session_id: sessionId,
              seq: 102.5 + i * 0.01,
              ts_ms: now(),
              payload: {
                kind: "exec_chunk",
                call_id: bgCallId,
                stream: line.stream,
                text: line.text,
              },
            })
          }, 1_900 + i * 150),
        )
      })
      // Exit marker arrives ~6s later, simulating a dev server that's killed
      // (or crashes) on its own — the row should flip to "exited" purely
      // from this chunk landing, with no further `tool_call_updated`.
      timers.push(
        window.setTimeout(() => {
          emit({
            session_id: sessionId,
            seq: 102.6,
            ts_ms: now(),
            payload: {
              kind: "exec_chunk",
              call_id: bgCallId,
              stream: "stdout",
              text: "[process exited with code 0]\n",
            },
          })
        }, 6_500),
      )

      // Retry-events demo: simulate the engine hitting a dropped connection
      // right after the shell tool settles, so the preview shows the
      // "Reconnecting" banner (TurnTimeline) replace the plain Working
      // indicator for a bit before streaming resumes. `retry_scheduled` is
      // ephemeral (live broadcast only) — never appended to JSONL/replay by
      // a real engine, so this mock only ever emits it, never persists it.
      const retryAt = 1_650 + execLines.length * 150 + 200
      timers.push(
        window.setTimeout(() => {
          emit({
            session_id: sessionId,
            seq: 102.9,
            ts_ms: now(),
            payload: {
              kind: "retry_scheduled",
              attempt: 1,
              max_attempts: 10,
              delay_ms: 3_000,
              error: "connection reset while streaming (network loss)",
            },
          })
        }, retryAt),
      )

      const reply = "Preview mock reply — layout changes look good."
      const chunks = [
        "Preview mock reply",
        " — layout changes",
        " look good.",
      ]
      const replyStart = retryAt + 1_500
      for (let i = 0; i < chunks.length; i++) {
        const chunk = chunks[i]
        const delay = replyStart + i * 180
        timers.push(
          window.setTimeout(() => {
            emit({
              session_id: sessionId,
              seq: 104 + i,
              ts_ms: now(),
              payload: {
                kind: "markdown_delta",
                message_id: messageId,
                text: chunk,
              },
            })
          }, delay),
        )
      }

      timers.push(
        window.setTimeout(() => {
          emit({
            session_id: sessionId,
            seq: 110,
            ts_ms: now(),
            payload: {
              kind: "assistant_message",
              message_id: messageId,
              model: input.model ?? "anthropic/claude-sonnet-4",
              content: [{ type: "markdown", text: reply }],
            },
          })

          emit({
            session_id: sessionId,
            seq: 111,
            ts_ms: now(),
            payload: {
              kind: "turn_completed",
              turn_id: turnId,
              summary: {
                turn_id: turnId,
                stop_reason: "end_turn",
                usage: {
                  input: 86_000,
                  output: 320,
                  cache_read: 71_000,
                  cache_write: 4_200,
                },
                cost_usd: 0.0089,
                num_model_calls: 2,
                num_tool_calls: 1,
                duration_ms: 2_300,
              },
            },
          })
          pendingTurns.delete(sessionId)
        }, replyStart + chunks.length * 180),
      )

      pendingTurns.set(sessionId, { timers, turnId, startedAt: ts })

      return {
        turn_id: turnId,
        stop_reason: "end_turn",
        usage: { input: 86_000, output: 320 },
        num_model_calls: 2,
        num_tool_calls: 1,
        duration_ms: 2_300,
      } as TurnSummary as T
    }
    case "routines_list":
      return mockRoutines as T
    case "routines_upsert": {
      const routine = (args?.routine ?? {}) as RoutineDto
      mockRoutines = [
        ...mockRoutines.filter((r) => r.id !== routine.id),
        routine,
      ].sort((a, b) => a.id.localeCompare(b.id))
      saveMockRoutines()
      return undefined as T
    }
    case "routines_remove": {
      const id = String(args?.id ?? "")
      mockRoutines = mockRoutines.filter((r) => r.id !== id)
      saveMockRoutines()
      delete mockRoutineHistory[id]
      saveMockRoutineHistory()
      return undefined as T
    }
    case "routines_run": {
      const id = String(args?.id ?? "")
      window.setTimeout(() => {
        const prev = mockRoutineHistory[id] ?? []
        mockRoutineHistory = {
          ...mockRoutineHistory,
          [id]: [
            ...prev,
            {
              sessionId: `preview-routine-${id}-${now()}`,
              startedMs: now(),
              stopReason: "completed",
              iterations: 3,
            },
          ],
        }
        saveMockRoutineHistory()
      }, 1_000)
      return undefined as T
    }
    case "routines_history": {
      const id = String(args?.id ?? "")
      return (mockRoutineHistory[id] ?? []) as T
    }
    case "mcp_list":
      return [...mockMcpServers].sort((a, b) => a.id.localeCompare(b.id)) as T
    case "mcp_upsert": {
      const server = (args?.server ?? {}) as McpServerDto
      mockMcpServers = [
        ...mockMcpServers.filter((s) => s.id !== server.id),
        server,
      ].sort((a, b) => a.id.localeCompare(b.id))
      saveMockMcpServers()
      return undefined as T
    }
    case "mcp_remove": {
      const id = String(args?.id ?? "")
      mockMcpServers = mockMcpServers.filter((s) => s.id !== id)
      saveMockMcpServers()
      return undefined as T
    }
    case "mcp_test": {
      const id = String(args?.id ?? "")
      const server = mockMcpServers.find((s) => s.id === id)
      if (!server) throw new Error(`server \`${id}\` not found`)
      // Preview stub: pretend every configured server connects fine and
      // exposes a couple of demo tools, keyed off the command name.
      return [`${server.command}_ping`, `${server.command}_status`] as T
    }
    case "memory_list": {
      purgeMockMemories()
      const sorted = [...mockMemories].sort(
        (a, b) => (b.updatedAtMs ?? 0) - (a.updatedAtMs ?? 0),
      )
      return sorted.map((m) => ({
        ...m,
        content: undefined,
        expiresAtMs: mockMemoryExpiry[m.id],
      })) as T
    }
    case "memory_get": {
      const id = String(args?.id ?? "")
      const memory = mockMemories.find((m) => m.id === id)
      if (!memory) throw new Error(`memory \`${id}\` not found`)
      return { ...memory, expiresAtMs: mockMemoryExpiry[id] } as T
    }
    case "memory_remove": {
      const id = String(args?.id ?? "")
      mockMemories = mockMemories.filter((m) => m.id !== id)
      delete mockMemoryExpiry[id]
      saveMockMemories()
      saveMockMemoryExpiry()
      return undefined as T
    }
    case "memory_set_expiry": {
      const id = String(args?.id ?? "")
      const expiresAtMs = args?.expiresAtMs as number | undefined
      if (expiresAtMs == null) {
        delete mockMemoryExpiry[id]
      } else {
        mockMemoryExpiry[id] = expiresAtMs
      }
      saveMockMemoryExpiry()
      return undefined as T
    }
    case "project_memory_list": {
      const cwd = String(args?.cwd ?? "")
      purgeMockProjectMemories(cwd)
      const list = mockProjectMemories[cwd] ?? []
      const expiry = mockProjectMemoryExpiry[cwd] ?? {}
      const sorted = [...list].sort(
        (a, b) => (b.updatedAtMs ?? 0) - (a.updatedAtMs ?? 0),
      )
      return sorted.map((m) => ({
        ...m,
        content: undefined,
        expiresAtMs: expiry[m.id],
      })) as T
    }
    case "project_memory_get": {
      const cwd = String(args?.cwd ?? "")
      const id = String(args?.id ?? "")
      const memory = (mockProjectMemories[cwd] ?? []).find((m) => m.id === id)
      if (!memory) throw new Error(`memory \`${id}\` not found`)
      return {
        ...memory,
        expiresAtMs: mockProjectMemoryExpiry[cwd]?.[id],
      } as T
    }
    case "project_memory_remove": {
      const cwd = String(args?.cwd ?? "")
      const id = String(args?.id ?? "")
      mockProjectMemories = {
        ...mockProjectMemories,
        [cwd]: (mockProjectMemories[cwd] ?? []).filter((m) => m.id !== id),
      }
      const nextExpiry = { ...(mockProjectMemoryExpiry[cwd] ?? {}) }
      delete nextExpiry[id]
      mockProjectMemoryExpiry = { ...mockProjectMemoryExpiry, [cwd]: nextExpiry }
      saveMockProjectMemories()
      saveMockProjectMemoryExpiry()
      return undefined as T
    }
    case "project_memory_set_expiry": {
      const cwd = String(args?.cwd ?? "")
      const id = String(args?.id ?? "")
      const expiresAtMs = args?.expiresAtMs as number | undefined
      const nextExpiry = { ...(mockProjectMemoryExpiry[cwd] ?? {}) }
      if (expiresAtMs == null) {
        delete nextExpiry[id]
      } else {
        nextExpiry[id] = expiresAtMs
      }
      mockProjectMemoryExpiry = { ...mockProjectMemoryExpiry, [cwd]: nextExpiry }
      saveMockProjectMemoryExpiry()
      return undefined as T
    }
    case "terminal_create": {
      const id = `term-${++mockTerminalSeq}`
      const cwd = String(args?.cwd ?? "/Users/preview/project")
      const createdAtMs = now()
      mockTerminals.set(id, { cwd, createdAtMs, lineBuf: "" })
      window.setTimeout(() => emitTerminalOutput({ id, data: TERM_PROMPT }), 50)
      return { id, cwd, createdAtMs } as TerminalInfo as T
    }
    case "terminal_write": {
      const id = String(args?.id ?? "")
      const data = String(args?.data ?? "")
      const term = mockTerminals.get(id)
      if (!term) return undefined as T
      for (const ch of data) {
        if (ch === "\r" || ch === "\n") {
          const line = term.lineBuf
          term.lineBuf = ""
          runMockLine(id, line)
        } else if (ch === "\x7f") {
          if (term.lineBuf.length > 0) {
            term.lineBuf = term.lineBuf.slice(0, -1)
            emitTerminalOutput({ id, data: "\b \b" })
          }
        } else {
          term.lineBuf += ch
          emitTerminalOutput({ id, data: ch })
        }
      }
      return undefined as T
    }
    case "terminal_resize":
      return undefined as T
    case "terminal_kill": {
      const id = String(args?.id ?? "")
      mockTerminals.delete(id)
      emitTerminalExit({ id, exitCode: 0 })
      return undefined as T
    }
    case "terminal_list":
      return Array.from(mockTerminals.entries()).map(([id, t]) => ({
        id,
        cwd: t.cwd,
        createdAtMs: t.createdAtMs,
      })) as TerminalInfo[] as T
    case "browser_open": {
      const url = normalizeBrowserUrl(String(args?.url ?? "https://www.google.com"))
      mockBrowserUrl = url
      emitBrowserLoadPulse(url)
      return undefined as T
    }
    case "browser_navigate": {
      const url = normalizeBrowserUrl(String(args?.url ?? mockBrowserUrl))
      mockBrowserUrl = url
      emitBrowserLoadPulse(url)
      return undefined as T
    }
    case "browser_back":
    case "browser_forward":
      return undefined as T
    case "browser_reload":
    case "browser_hard_reload": {
      emitBrowserLoadPulse(mockBrowserUrl)
      return undefined as T
    }
    case "browser_set_bounds":
    case "browser_set_visible":
    case "browser_close":
      return undefined as T
    // Native-only: opening a real DevTools window has no preview equivalent.
    // BrowserTab.tsx checks `isBrowserPreview()` and shows a toast instead of
    // calling this, but resolve cleanly here too in case it's ever invoked.
    case "browser_open_devtools":
      return undefined as T
    // Native-only: `clear_all_browsing_data` has no meaningful preview
    // equivalent (no real cookies/cache exist for the mock iframe) — no-op.
    case "browser_clear_data":
      return undefined as T
    // Native-only: `screencapture` has no preview equivalent. BrowserTab.tsx
    // checks `isBrowserPreview()` and shows a degradation toast instead of
    // calling this, but throw here too in case it's ever invoked directly.
    case "browser_screenshot":
      throw new Error("Screenshot unavailable in preview")
    default:
      throw new Error(`Browser preview mock: unhandled command "${cmd}"`)
  }
}

export const browserListenSessionEvents = (
  handler: (event: SessionEvent) => void,
): Promise<UnlistenFn> => {
  eventHandlers.add(handler)
  return Promise.resolve(() => {
    eventHandlers.delete(handler)
  })
}

// Terminal + Browser right-panel mocks (Vite preview only)

const terminalOutputHandlers = new Set<(e: TerminalOutputEvent) => void>()
const terminalExitHandlers = new Set<(e: TerminalExitEvent) => void>()

let mockTerminalSeq = 0
const mockTerminals = new Map<
  string,
  { cwd: string; createdAtMs: number; lineBuf: string }
>()

const TERM_PROMPT = "\r\n\x1b[36m❯\x1b[0m "

const emitTerminalOutput = (e: TerminalOutputEvent) => {
  for (const handler of terminalOutputHandlers) handler(e)
}

const emitTerminalExit = (e: TerminalExitEvent) => {
  for (const handler of terminalExitHandlers) handler(e)
}

const runMockLine = (id: string, rawLine: string) => {
  const term = mockTerminals.get(id)
  const cwd = term?.cwd ?? "/Users/preview/project"
  const line = rawLine.trim()
  const cmd = line.split(/\s+/)[0] ?? ""

  let output: string
  if (line === "") {
    output = TERM_PROMPT
    emitTerminalOutput({ id, data: output })
    return
  } else if (cmd === "ls") {
    output = "\r\nsrc  package.json  README.md"
  } else if (cmd === "pwd") {
    output = "\r\n" + cwd
  } else if (cmd === "help") {
    output = "\r\nmock shell — try ls, pwd, clear, help"
  } else if (cmd === "clear") {
    emitTerminalOutput({ id, data: "\x1b[2J\x1b[H" })
    emitTerminalOutput({ id, data: "\x1b[36m❯\x1b[0m " })
    return
  } else {
    output = "\r\nzsh: command not found: " + cmd
  }

  emitTerminalOutput({ id, data: output + TERM_PROMPT })
}

export const browserListenTerminalOutput = (
  handler: (e: TerminalOutputEvent) => void,
): Promise<UnlistenFn> => {
  terminalOutputHandlers.add(handler)
  return Promise.resolve(() => {
    terminalOutputHandlers.delete(handler)
  })
}

export const browserListenTerminalExit = (
  handler: (e: TerminalExitEvent) => void,
): Promise<UnlistenFn> => {
  terminalExitHandlers.add(handler)
  return Promise.resolve(() => {
    terminalExitHandlers.delete(handler)
  })
}

const browserStateHandlers = new Set<(e: BrowserStateEvent) => void>()
let mockBrowserUrl = ""

const normalizeBrowserUrl = (url: string): string =>
  url.includes("://") ? url : `https://${url}`

const emitBrowserState = (e: BrowserStateEvent) => {
  for (const handler of browserStateHandlers) handler(e)
}

/** Deterministic "load failure" URL for previewing the load-error page —
 * a real request to this host/port refuses to connect, so it stands in for
 * native load-failure detection (chrome-error / connection-refused probe
 * after `PageLoadEvent::Finished` in `browser.rs`). */
const FAILING_MOCK_HOST = "localhost:1"

const emitBrowserLoadPulse = (url: string) => {
  emitBrowserState({
    url,
    title: null,
    loading: true,
    canGoBack: true,
    canGoForward: true,
  })
  window.setTimeout(() => {
    let host = url
    try {
      host = new URL(url).host
    } catch {
      // Keep the raw url as the host fallback.
    }
    const failed = host === FAILING_MOCK_HOST
    emitBrowserState({
      url,
      title: null,
      loading: false,
      canGoBack: true,
      canGoForward: true,
      error: failed
        ? { host, message: `${host} refused to connect` }
        : null,
    })
  }, 300)
}

export const browserListenBrowserState = (
  handler: (e: BrowserStateEvent) => void,
): Promise<UnlistenFn> => {
  browserStateHandlers.add(handler)
  return Promise.resolve(() => {
    browserStateHandlers.delete(handler)
  })
}

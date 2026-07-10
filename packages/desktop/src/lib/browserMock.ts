import type { UnlistenFn } from "@tauri-apps/api/event"
import type {
  BrowserStateEvent,
  BuiltinProvider,
  CreateSessionInput,
  ModelInfoDto,
  PromptCommandInput,
  ProviderConfigView,
  RoutineDto,
  RoutineRunRecordDto,
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
  title: "Cursor chat redesign",
  agent_id: "native",
  depth: 0,
  cwd: "/Users/preview/flex-app",
  model: "anthropic/claude-sonnet-4",
  fallback_models: [],
  created_at_ms: now() - 3_600_000,
  updated_at_ms: now() - 120_000,
  ...overrides,
})

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
]

let configured = true
const eventHandlers = new Set<(event: SessionEvent) => void>()
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
const loadMockPlugins = (): { search: boolean; learning: boolean; verifier: boolean } => {
  try {
    const raw = window.sessionStorage.getItem("mock-plugins")
    if (raw) return JSON.parse(raw) as ReturnType<typeof loadMockPlugins>
  } catch {
    // Fall through to defaults.
  }
  return { search: true, learning: false, verifier: false }
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
})

/** In-flight mock turns — cancel clears timers and emits turn_completed. */
const pendingTurns = new Map<
  string,
  { timers: number[]; turnId: string; startedAt: number }
>()

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

const timelineEvents = (sessionId: string): SessionEvent[] => {
  if (sessionId === "preview-session-2") return []
  const ts = now() - 90_000

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
            text: "Tighten the chat shell spacing and hierarchy, then align it with the Cursor agents window design.",
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
    case "git_branch": {
      const cwd = String(args?.cwd ?? "")
      return (mockBranchByCwd[cwd] ?? "main") as T
    }
    case "git_list_branches":
      return mockBranches as T
    case "git_status":
      return [
        { path: "src/App.tsx", status: "M", added: 24, removed: 3 },
        { path: "src/styles/tokens.css", status: "M", added: 58, removed: 12 },
        { path: "src/pages/CustomizePage.tsx", status: "?" },
      ] as T
    case "git_diff": {
      const p = String(args?.path ?? "file")
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
        "",
      ].join("\n") as T
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
      ] as BuiltinProvider[] as T
    case "list_models":
      return models as T
    case "list_providers":
      return ["anthropic", "openai"] as T
    case "validate_provider":
      return models as T
    case "save_provider_config": {
      configured = true
      const input = args?.input as
        | { plugins?: { search: boolean; learning: boolean; verifier: boolean } }
        | undefined
      if (input?.plugins) {
        mockPlugins = { ...input.plugins }
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
    case "is_isolated":
      return (String(args?.sessionId) === "preview-session-1") as T
    case "workspace_status":
      return (String(args?.sessionId) === "preview-session-1"
        ? { filesChanged: 3, summary: "src/app.ts, src/index.css, +1 more" }
        : null) as T
    case "integrate_session":
      return { status: "empty" } as T
    case "discard_session":
    case "revert":
      return undefined as T
    case "list_sessions":
      return sessions as T
    case "create_session": {
      const input = (args?.input ?? {}) as CreateSessionInput
      const session = demoSession({
        id: `preview-session-${sessions.length + 1}`,
        title: input.title ?? "New Agent",
        cwd: input.cwd ?? "/Users/preview/project",
        model: input.model ?? "anthropic/claude-sonnet-4",
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
    case "replay":
      return timelineEvents(String(args?.sessionId ?? "")) as T
    case "subscribe_session":
    case "unsubscribe_session":
      return undefined as T
    case "cancel": {
      const sessionId = String(args?.sessionId ?? "")
      const pending = pendingTurns.get(sessionId)
      if (!pending) return undefined as T
      clearPendingTurn(sessionId)
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
      return undefined as T
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
      const ts = now()
      const turnId = `turn-${ts}`
      const messageId = `m-asst-${ts}`
      const isPlan = input.permissionMode === "plan"
      const wantsPermission = /\b(permission|allow|approve)\b/i.test(input.text)
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

      const reply = "Preview mock reply — layout changes look good."
      const chunks = [
        "Preview mock reply",
        " — layout changes",
        " look good.",
      ]
      for (let i = 0; i < chunks.length; i++) {
        const chunk = chunks[i]
        const delay = 1_600 + i * 180
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
        }, 2_300),
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
    case "browser_reload": {
      emitBrowserLoadPulse(mockBrowserUrl)
      return undefined as T
    }
    case "browser_set_bounds":
    case "browser_set_visible":
    case "browser_close":
      return undefined as T
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

const emitBrowserLoadPulse = (url: string) => {
  emitBrowserState({
    url,
    title: null,
    loading: true,
    canGoBack: true,
    canGoForward: true,
  })
  window.setTimeout(() => {
    emitBrowserState({
      url,
      title: null,
      loading: false,
      canGoBack: true,
      canGoForward: true,
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

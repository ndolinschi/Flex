import { invoke as tauriInvoke } from "@tauri-apps/api/core"
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event"
import { isBrowserPreview, NATIVE_APP_REQUIRED } from "./browserPreview"
import { log, truncateForLog } from "./debug/log"
import type {
  BackgroundProcessDto,
  BrowserStateEvent,
  BuiltinProvider,
  CommandInfoDto,
  ChatgptAuthStart,
  ChatgptAuthStatus,
  CopilotAuthStart,
  CopilotAuthStatus,
  CreateSessionInput,
  FileHit,
  GitStatusSummary,
  IndexRebuildResult,
  IndexStatus,
  McpServerDto,
  MemoryEntryDto,
  ModelInfoDto,
  PromptCommandInput,
  ProviderConfigView,
  ProviderProfileInput,
  ProviderProfileView,
  RespondPermissionInput,
  RespondQuestionInput,
  ReviewPatchTarget,
  RoutineDto,
  RoutineRunRecordDto,
  SaveProviderConfigInput,
  SecretStorageMode,
  SessionEvent,
  SessionMeta,
  TerminalExitEvent,
  TerminalInfo,
  TerminalOutputEvent,
  TurnSummary,
  UpdateSessionInput,
  WorkspaceStatusDto,
  BrowserDesignEvent,
} from "./types"

/** Single choke point for every native Tauri `invoke`. Browser preview has
 * no backend — reject clearly instead of simulating IPC. Logs the command
 * name, truncated args, duration, and (on failure) the error under the
 * "ipc" namespace — no-op when debug logging is off (see
 * `log.ts::isDebugEnabled`). */
const invoke = async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
  const startedAt = performance.now()
  if (isBrowserPreview()) {
    const err = new Error(NATIVE_APP_REQUIRED)
    log.error("ipc", `${cmd} failed`, {
      args: args ? truncateForLog(args) : undefined,
      durationMs: Math.round(performance.now() - startedAt),
      error: err.message,
    })
    throw err
  }
  try {
    const result = await tauriInvoke<T>(cmd, args)
    log.debug("ipc", cmd, {
      args: args ? truncateForLog(args) : undefined,
      durationMs: Math.round(performance.now() - startedAt),
    })
    return result
  } catch (err) {
    log.error("ipc", `${cmd} failed`, {
      args: args ? truncateForLog(args) : undefined,
      durationMs: Math.round(performance.now() - startedAt),
      error: err instanceof Error ? err.message : String(err),
    })
    throw err
  }
}

export const hello = (): Promise<unknown> => invoke("hello")

export const getProviderConfig = (): Promise<ProviderConfigView> =>
  invoke("get_provider_config")

export const listBuiltinProviders = (): Promise<BuiltinProvider[]> =>
  invoke("list_builtin_providers")

export const validateProvider = (
  input: SaveProviderConfigInput,
): Promise<ModelInfoDto[]> => invoke("validate_provider", { input })

export const saveProviderConfig = (
  input: SaveProviderConfigInput,
): Promise<ProviderConfigView> => invoke("save_provider_config", { input })

/** Switch the secret storage backend (migrates the master key — see
 * `src-tauri/src/secrets.rs::SecretsStore::switch_mode`). Rejects with an
 * error message on migration failure; the previous backend stays intact. */
export const setSecretStorage = (
  mode: SecretStorageMode,
): Promise<ProviderConfigView> => invoke("set_secret_storage", { mode })

/** OS platform family (`"macos" | "windows" | "linux" | ...`), via
 * `@tauri-apps/plugin-os`'s `type()` — gates the Security section's "System
 * Keychain" option to macOS only (the backend rejects it elsewhere; see
 * `secrets.rs`). */
export const platformType = async (): Promise<string> => {
  if (isBrowserPreview()) throw new Error(NATIVE_APP_REQUIRED)
  const { type } = await import("@tauri-apps/plugin-os")
  return type()
}

// Named provider connections ("profiles") — see `src-tauri/src/config.rs`'s
// "Named provider connections" section for the backend contract.

export const profilesList = (): Promise<ProviderProfileView[]> =>
  invoke("profiles_list")

export const profileUpsert = (
  profile: ProviderProfileInput,
): Promise<ProviderProfileView> => invoke("profile_upsert", { profile })

export const profileRemove = (id: string): Promise<void> =>
  invoke("profile_remove", { id })

export const profileActivate = (id: string): Promise<ProviderConfigView> =>
  invoke("profile_activate", { id })

/** Validate a connection using exactly the form's current values — fixes the
 * bug where Validate ignored a freshly pasted key and fell back to env/stored
 * config (see `commands::validate_profile`'s doc comment). */
export const validateProfile = (
  input: ProviderProfileInput,
): Promise<ModelInfoDto[]> => invoke("validate_profile", { input })

/** Whether a GitHub Copilot OAuth token is discoverable (env or apps.json). */
export const copilotAuthStatus = (): Promise<CopilotAuthStatus> =>
  invoke("copilot_auth_status")

/** Start a GitHub device-code sign-in; returns the user code to show. */
export const copilotAuthStart = (): Promise<CopilotAuthStart> =>
  invoke("copilot_auth_start")

/** Poll until the user confirms the code on github.com. */
export const copilotAuthWait = (
  sessionId: string,
): Promise<CopilotAuthStatus> => invoke("copilot_auth_wait", { sessionId })

/** Cancel an in-flight Copilot device-flow wait. */
export const copilotAuthCancel = (sessionId: string): Promise<void> =>
  invoke("copilot_auth_cancel", { sessionId })

/** Whether ChatGPT Plus/Pro OAuth tokens are discoverable. */
export const chatgptAuthStatus = (): Promise<ChatgptAuthStatus> =>
  invoke("chatgpt_auth_status")

/** Start a ChatGPT headless device-code sign-in; returns the user code. */
export const chatgptAuthStart = (): Promise<ChatgptAuthStart> =>
  invoke("chatgpt_auth_start")

/** Poll until the user confirms the code on auth.openai.com. */
export const chatgptAuthWait = (
  sessionId: string,
): Promise<ChatgptAuthStatus> => invoke("chatgpt_auth_wait", { sessionId })

/** Cancel an in-flight ChatGPT OAuth wait. */
export const chatgptAuthCancel = (sessionId: string): Promise<void> =>
  invoke("chatgpt_auth_cancel", { sessionId })

export const listModels = (): Promise<ModelInfoDto[]> => invoke("list_models")

export const listProviders = (): Promise<string[]> => invoke("list_providers")

export const listCommands = (): Promise<CommandInfoDto[]> =>
  invoke("list_commands")

export const createSession = (input: CreateSessionInput): Promise<SessionMeta> =>
  invoke("create_session", { input })

export const listSessions = (): Promise<SessionMeta[]> => invoke("list_sessions")

export const sessionMeta = (sessionId: string): Promise<SessionMeta> =>
  invoke("session_meta", { sessionId })

export const resumeSession = (sessionId: string): Promise<void> =>
  invoke("resume_session", { sessionId })

export const updateSession = (
  sessionId: string,
  patch: UpdateSessionInput,
): Promise<SessionMeta> => invoke("update_session", { sessionId, patch })

/** One-shot semantic title suggestion (2-5 words) for a session's first
 * turn, generated via the session's own model — see
 * `commands::suggest_session_title` in src-tauri. Callers should treat any
 * rejection as non-fatal and just keep the existing title. */
export const suggestSessionTitle = (
  sessionId: string,
  promptText: string,
): Promise<string> =>
  invoke("suggest_session_title", { sessionId, promptText })

export const deleteSession = (sessionId: string): Promise<void> =>
  invoke("delete_session", { sessionId })

export const replay = (
  sessionId: string,
  fromSeq?: number,
): Promise<SessionEvent[]> => invoke("replay", { sessionId, fromSeq })

export const subscribeSession = (sessionId: string): Promise<void> =>
  invoke("subscribe_session", { sessionId })

export const unsubscribeSession = (sessionId: string): Promise<void> =>
  invoke("unsubscribe_session", { sessionId })

export const prompt = (input: PromptCommandInput): Promise<TurnSummary> =>
  invoke("prompt", { input })

export const cancel = (sessionId: string): Promise<void> =>
  invoke("cancel", { sessionId })

/** List background processes (started via `Bash`'s `run_in_background`) for
 * a session — see `background_list` in `src-tauri/src/commands.rs`. */
export const backgroundList = (
  sessionId: string,
): Promise<BackgroundProcessDto[]> => invoke("background_list", { sessionId })

/** Kill one background process by id — see `background_kill` in
 * `src-tauri/src/commands.rs`. */
export const backgroundKill = (
  sessionId: string,
  processId: string,
): Promise<void> => invoke("background_kill", { sessionId, processId })

/** Ask a still-running foreground shell call to move to the background (see
 * `MOVE-TO-BACKGROUND`) — see `background_demote` in
 * `src-tauri/src/commands.rs`. Resolves `false` (not a rejection) when
 * there's nothing to do: the call already finished, or the session's
 * execution backend doesn't support demote — callers should treat that the
 * same as `true` (silently no visible change) rather than surface an error. */
export const backgroundDemote = (
  sessionId: string,
  callId: string,
): Promise<boolean> => invoke("background_demote", { sessionId, callId })

export const respondPermission = (input: RespondPermissionInput): Promise<void> =>
  invoke("respond_permission", { input })

/** Push a permission-mode change into an in-flight turn (e.g. composer
 * bypass shield mid-run). Pass `null`/empty to clear. */
export const setTurnPermissionMode = (
  sessionId: string,
  mode: string | null,
): Promise<void> => invoke("set_turn_permission_mode", { sessionId, mode })

export const respondQuestion = (input: RespondQuestionInput): Promise<void> =>
  invoke("respond_question", { input })

export const isConfigured = (): Promise<boolean> => invoke("is_configured")

/** Whether `cwd` is inside a git repository at all (`git rev-parse
 * --git-dir`), regardless of commit history. Gates the entire git chrome —
 * branch pill, changes badge, commit bar, Changes tab content — so a
 * non-git folder shows none of it. A repo with an unborn HEAD (freshly
 * `git init`-ed, no commits yet) still returns true. */
export const gitIsRepo = (cwd: string): Promise<boolean> =>
  invoke("git_is_repo", { cwd })

/** Whether the repo at `cwd` has any configured remotes (`git remote`).
 * Gates Commit vs Commit & Push — no remote means push is unavailable. */
export const gitHasRemote = (cwd: string): Promise<boolean> =>
  invoke("git_has_remote", { cwd })

export const gitBranch = (cwd: string): Promise<string | null> =>
  invoke("git_branch", { cwd })

export const gitListBranches = (cwd: string): Promise<string[]> =>
  invoke("git_list_branches", { cwd })

export const gitCheckout = (cwd: string, branch: string): Promise<void> =>
  invoke("git_checkout", { cwd, branch })

export const gitStatus = (cwd: string): Promise<GitStatusSummary> =>
  invoke("git_status", { cwd })

/** Files changed since the session's baseline (captured at create/resume).
 * Gracefully degrades to full `git_status` for isolated sessions, missing
 * baselines, or a moved HEAD. Server-capped + summarized — see
 * `GitStatusSummary`'s doc comment. */
export const gitStatusSinceBaseline = (
  sessionId: string,
): Promise<GitStatusSummary> => invoke("git_status_since_baseline", { sessionId })

export const gitDiff = (cwd: string, path: string): Promise<string> =>
  invoke("git_diff", { cwd, path })

/** Stage all + commit in the session's cwd (non-isolated only). Returns the
 * resulting commit's short SHA. */
export const gitCommit = (
  sessionId: string,
  message: string,
): Promise<string> => invoke("git_commit", { sessionId, message })

/** Push the current branch in the session's cwd (non-isolated only). */
export const gitPush = (sessionId: string): Promise<void> =>
  invoke("git_push", { sessionId })

// ── Commit center (Changes tab, spec #48) ──────────────────────────────
// Selective staging + commit/push/branch/PR flow. All reject isolated
// sessions the same way `gitCommit`/`gitPush` do (see their doc comments).

/** Stage exactly `paths` (not the whole repo) and commit. Returns the
 * resulting commit's short SHA. */
export const gitCommitPaths = (
  sessionId: string,
  message: string,
  paths: string[],
): Promise<string> =>
  invoke("git_commit_paths", { sessionId, message, paths })

/** Commit the selected files, then push (auto-creates the upstream with
 * `-u origin <branch>` on the branch's first push). */
export const gitCommitAndPush = (
  sessionId: string,
  message: string,
  paths: string[],
): Promise<string> =>
  invoke("git_commit_and_push", { sessionId, message, paths })

/** Create + check out a new local branch off HEAD, then commit the
 * selected files to it. */
export const gitCreateBranchAndCommit = (
  sessionId: string,
  branch: string,
  message: string,
  paths: string[],
): Promise<string> =>
  invoke("git_create_branch_and_commit", { sessionId, branch, message, paths })

export type CreatePrOutcome = {
  commitSha: string
  prUrl: string | null
  /** Set when the commit+push succeeded but the PR step itself was skipped
   * (e.g. `gh` missing/unauthenticated) — show as a non-fatal toast, not an
   * error; the work is not lost. */
  degradedReason: string | null
}

/** Commit the selected files, push, then `gh pr create --fill` (or with an
 * explicit title/body). Gracefully degrades — see `CreatePrOutcome.degradedReason`
 * — instead of throwing when the GitHub CLI is unavailable. */
export const gitCreatePr = (
  sessionId: string,
  message: string,
  paths: string[],
  title?: string,
  body?: string,
): Promise<CreatePrOutcome> =>
  invoke("git_create_pr", { sessionId, message, paths, title, body })

export type BranchPrInfo = {
  number: number
  title: string
  url: string
  state: string
  checksSummary: string
}

export type BranchPrStatus = {
  ghAvailable: boolean
  pr: BranchPrInfo | null
}

/** Current-branch PR + CI summary via `gh pr view`. Returns `pr: null` when
 * there is no PR or `gh` is unavailable — safe to poll from the Changes UI. */
export const gitPrStatus = (cwd: string): Promise<BranchPrStatus> =>
  invoke("git_pr_status", { cwd })

export type PrDraft = {
  title: string
  body: string
}

/** Prefill title/body for the Create PR dialog (latest commit + ahead range). */
export const gitPrDraft = (cwd: string): Promise<PrDraft> =>
  invoke("git_pr_draft", { cwd })

/** Create a PR for the current branch without a new commit. Pass title/body
 * to override `gh --fill`; omit both to fill from commits. If a PR already
 * exists, returns its URL. */
export const gitCreatePrForBranch = (
  cwd: string,
  title?: string,
  body?: string,
): Promise<CreatePrOutcome> =>
  invoke("git_create_pr_for_branch", { cwd, title, body })

/** One-shot commit-message suggestion from a diff summary, via the
 * session's own model — see `commands::suggest_commit_message`. Callers
 * should treat any rejection as non-fatal and just leave the message box
 * empty. */
export const suggestCommitMessage = (
  sessionId: string,
  diffSummary: string,
): Promise<string> =>
  invoke("suggest_commit_message", { sessionId, diffSummary })

/** Fuzzy file/folder search. Pass `includeIgnored` for the human Files search
 * so `.env` and other gitignored paths appear; omit for composer `@` (default). */
export const listFiles = (
  cwd: string,
  query: string,
  includeIgnored = false,
): Promise<FileHit[]> =>
  invoke("list_files", { cwd, query, includeIgnored })

/** Immediate children of `relativeDir` under `cwd` ("" = workspace root).
 * Human Files tree: shows hidden + gitignored entries (e.g. `.env`). */
export const listDirChildren = (
  cwd: string,
  relativeDir: string,
): Promise<FileHit[]> => invoke("list_dir_children", { cwd, relativeDir })

export type DbEngine = "sqlite" | "postgres" | "mysql"

export type DbConnectionSpec = {
  id: string
  name: string
  engine: DbEngine
  target: string
  /** Normalized project cwd; required on save. Legacy entries may be "". */
  projectKey?: string
}

export type DbSchemaInfo = { name: string }

export type DbTableInfo = {
  schema: string
  name: string
  kind: string
}

export type DbQueryResult = {
  columns: string[]
  rows: unknown[][]
  truncated: boolean
  rowCount: number
}

export type DbMentionHit = {
  name: string
  path: string
  insertText: string
}

export const dbListConnections = (
  projectKey: string,
): Promise<DbConnectionSpec[]> =>
  invoke("db_list_connections", { projectKey })

export const dbUpsertConnection = (
  spec: DbConnectionSpec,
): Promise<DbConnectionSpec> => invoke("db_upsert_connection", { spec })

export const dbRemoveConnection = (id: string): Promise<void> =>
  invoke("db_remove_connection", { id })

export const dbConnect = (id: string): Promise<DbConnectionSpec> =>
  invoke("db_connect", { id })

export const dbDisconnect = (id: string): Promise<void> =>
  invoke("db_disconnect", { id })

export const dbActiveConnection = (
  projectKey: string,
): Promise<DbConnectionSpec | null> =>
  invoke("db_active_connection", { projectKey })

export const dbListSchemas = (id: string): Promise<DbSchemaInfo[]> =>
  invoke("db_list_schemas", { id })

export const dbListTables = (
  id: string,
  schema?: string,
): Promise<DbTableInfo[]> => invoke("db_list_tables", { id, schema })

export const dbPreviewTable = (
  id: string,
  schema: string,
  table: string,
  limit?: number,
  offset?: number,
): Promise<DbQueryResult> =>
  invoke("db_preview_table", { id, schema, table, limit, offset })

export const dbQuery = (id: string, sql: string): Promise<DbQueryResult> =>
  invoke("db_query", { id, sql })

export const dbMentionTables = (
  query: string,
  projectKey: string,
): Promise<DbMentionHit[]> =>
  invoke("db_mention_tables", { query, projectKey })

export const isIsolated = (sessionId: string): Promise<boolean> =>
  invoke("is_isolated", { sessionId })

export const workspaceStatus = (
  sessionId: string,
): Promise<WorkspaceStatusDto | null> =>
  invoke("workspace_status", { sessionId })

export const integrateSession = (sessionId: string): Promise<unknown> =>
  invoke("integrate_session", { sessionId })

export const discardIsolatedSession = (sessionId: string): Promise<void> =>
  invoke("discard_session", { sessionId })

export const revertSnapshot = (
  sessionId: string,
  snapshotId: string,
): Promise<void> => invoke("revert", { sessionId, snapshotId })

export const listenSessionEvents = (
  handler: (event: SessionEvent) => void,
): Promise<UnlistenFn> => {
  if (isBrowserPreview()) return Promise.resolve(() => {})
  return tauriListen<SessionEvent>("session-event", (e) => {
    handler(e.payload)
  })
}

export const routinesList = (): Promise<RoutineDto[]> => invoke("routines_list")

export const routinesUpsert = (routine: RoutineDto): Promise<void> =>
  invoke("routines_upsert", { routine })

export const routinesRemove = (id: string): Promise<void> =>
  invoke("routines_remove", { id })

export const routinesRun = (id: string): Promise<void> =>
  invoke("routines_run", { id })

export const routinesHistory = (id: string): Promise<RoutineRunRecordDto[]> =>
  invoke("routines_history", { id })

export const mcpList = (): Promise<McpServerDto[]> => invoke("mcp_list")

export const mcpUpsert = (server: McpServerDto): Promise<void> =>
  invoke("mcp_upsert", { server })

export const mcpRemove = (id: string): Promise<void> =>
  invoke("mcp_remove", { id })

/** Connect to a saved server and return its discovered tool names — throws
 * (via `toInvokeError`) on connection failure. */
export const mcpTest = (id: string): Promise<string[]> =>
  invoke("mcp_test", { id })

export const memoryList = (): Promise<MemoryEntryDto[]> => invoke("memory_list")

export const memoryGet = (id: string): Promise<MemoryEntryDto> =>
  invoke("memory_get", { id })

export const memoryRemove = (id: string): Promise<void> =>
  invoke("memory_remove", { id })

/** Set (or clear, passing `undefined`) a global memory entry's expiry.
 * Backed by a sidecar `expiry.json` next to the `.md` notes — see
 * `src-tauri/src/commands.rs`'s "Memory expiry" section. */
export const memorySetExpiry = (
  id: string,
  expiresAtMs: number | undefined,
): Promise<void> => invoke("memory_set_expiry", { id, expiresAtMs })

/** Per-project memory — same shape as the global memory above, but backed by
 * `<cwd>/.agent/memory/*.md` instead of the global store. Writes are still
 * global-only for now (the agent doesn't yet author into a project's
 * memory dir), so this surface is read + delete + expiry only. */
export const projectMemoryList = (cwd: string): Promise<MemoryEntryDto[]> =>
  invoke("project_memory_list", { cwd })

export const projectMemoryGet = (
  cwd: string,
  id: string,
): Promise<MemoryEntryDto> => invoke("project_memory_get", { cwd, id })

export const projectMemoryRemove = (cwd: string, id: string): Promise<void> =>
  invoke("project_memory_remove", { cwd, id })

export const projectMemorySetExpiry = (
  cwd: string,
  id: string,
  expiresAtMs: number | undefined,
): Promise<void> =>
  invoke("project_memory_set_expiry", { cwd, id, expiresAtMs })

export const terminalCreate = (cwd?: string): Promise<TerminalInfo> =>
  invoke("terminal_create", { cwd })

export const terminalWrite = (id: string, data: string): Promise<void> =>
  invoke("terminal_write", { id, data })

export const terminalResize = (
  id: string,
  cols: number,
  rows: number,
): Promise<void> => invoke("terminal_resize", { id, cols, rows })

export const terminalKill = (id: string): Promise<void> =>
  invoke("terminal_kill", { id })

export const terminalList = (): Promise<TerminalInfo[]> =>
  invoke("terminal_list")

export const browserOpen = (url?: string): Promise<void> =>
  invoke("browser_open", { url })

export const browserNavigate = (url: string): Promise<void> =>
  invoke("browser_navigate", { url })

export const browserBack = (): Promise<void> => invoke("browser_back")

export const browserForward = (): Promise<void> => invoke("browser_forward")

export const browserReload = (): Promise<void> => invoke("browser_reload")

export const browserSetBounds = (
  x: number,
  y: number,
  width: number,
  height: number,
): Promise<void> => invoke("browser_set_bounds", { x, y, width, height })

export const browserSetVisible = (visible: boolean): Promise<void> =>
  invoke("browser_set_visible", { visible })

export const browserClose = (): Promise<void> => invoke("browser_close")

/** Opens DevTools for the embedded browser's child webview only — never the
 * app's main webview. See `src-tauri/src/browser.rs::browser_open_devtools`. */
export const browserOpenDevtools = (): Promise<void> =>
  invoke("browser_open_devtools")

/** Re-navigates to the current URL to force a cache-bypassing reload — the
 * "…" menu's Hard Reload, distinct from the soft `browserReload` (plain
 * `webview.reload()`). See `src-tauri/src/browser.rs::browser_hard_reload`. */
export const browserHardReload = (): Promise<void> =>
  invoke("browser_hard_reload")

/** Clears cookies/cache/storage for the embedded browser's child webview via
 * wry's `clear_all_browsing_data`. Shipped as one action, not split
 * cookies/cache, since the underlying API has no such split. See
 * `src-tauri/src/browser.rs::browser_clear_data`. */
export const browserClearData = (): Promise<void> => invoke("browser_clear_data")

/** Captures a screenshot of the embedded browser's on-screen region (macOS
 * `screencapture -R`) and returns the temp PNG path. See
 * `src-tauri/src/browser.rs::browser_screenshot`. */
export const browserScreenshot = (): Promise<string> => invoke("browser_screenshot")

/** Enable/disable Design Mode (element picker) in the embedded browser. */
export const browserSetDesignMode = (enabled: boolean): Promise<void> =>
  invoke("browser_set_design_mode", { enabled })

export const listenTerminalOutput = (
  handler: (event: TerminalOutputEvent) => void,
): Promise<UnlistenFn> => {
  if (isBrowserPreview()) return Promise.resolve(() => {})
  return tauriListen<TerminalOutputEvent>("terminal-output", (e) => {
    handler(e.payload)
  })
}

export const listenTerminalExit = (
  handler: (event: TerminalExitEvent) => void,
): Promise<UnlistenFn> => {
  if (isBrowserPreview()) return Promise.resolve(() => {})
  return tauriListen<TerminalExitEvent>("terminal-exit", (e) => {
    handler(e.payload)
  })
}

export const listenBrowserState = (
  handler: (event: BrowserStateEvent) => void,
): Promise<UnlistenFn> => {
  if (isBrowserPreview()) return Promise.resolve(() => {})
  return tauriListen<BrowserStateEvent>("browser-state", (e) => {
    handler(e.payload)
  })
}

export const listenBrowserDesign = (
  handler: (event: BrowserDesignEvent) => void,
): Promise<UnlistenFn> => {
  if (isBrowserPreview()) return Promise.resolve(() => {})
  return tauriListen<BrowserDesignEvent>("browser-design-event", (e) => {
    handler(e.payload)
  })
}

// Per-file / per-hunk review actions (Changes tab Keep/Undo — see
// `src-tauri/src/commands.rs`'s "Review flow" section for the backend
// contract these wrap).

/** Revert one file to its pre-agent base state: isolated sessions restore
 * from the base repo's HEAD, non-isolated sessions restore from the repo's
 * own HEAD. Untracked files are deleted outright. */
export const reviewUndoFile = (
  sessionId: string,
  path: string,
): Promise<void> => invoke("review_undo_file", { sessionId, path })

/** Isolated sessions only: copy the worktree's current copy of `path` into
 * the base repo's working tree (or remove it there if deleted in the
 * worktree). Errors if the session isn't isolated — callers should hide
 * "Keep" entirely for non-isolated sessions rather than relying on this. */
export const reviewKeepFile = (
  sessionId: string,
  path: string,
): Promise<void> => invoke("review_keep_file", { sessionId, path })

/** Apply (or `reverse`-apply) a small unified-diff `patch` — built via
 * `buildPatch` from one or more parsed hunks — against either the session's
 * working dir (`"worktree"`, which is just the repo cwd for non-isolated
 * sessions) or its isolated base repo (`"base"`, errors if not isolated). */
export const reviewApplyPatch = (
  sessionId: string,
  patch: string,
  target: ReviewPatchTarget,
  reverse: boolean,
): Promise<void> =>
  invoke("review_apply_patch", { sessionId, patch, target, reverse })

/** Unified diff for one file against its pre-agent base state (base repo's
 * HEAD when isolated, else `HEAD`) — the Changes-tab equivalent of `gitDiff`
 * that stays correct once `integrate_session` has committed some agent
 * changes into an isolated worktree. */
export const reviewFileDiff = (
  sessionId: string,
  path: string,
): Promise<string> => invoke("review_file_diff", { sessionId, path })

export const toInvokeError = (err: unknown): string => {
  if (typeof err === "string") return err
  if (err instanceof Error) return err.message
  return "An unexpected error occurred"
}

/** Sidebar footer identity — best-effort local display name (`git config
 * user.name`, falling back to `$USER`, falling back to "User"). */
export type UserIdentityDto = {
  name: string
}

export const userIdentity = (): Promise<UserIdentityDto> =>
  invoke("user_identity")

/** Writes `content` to `relativePath` inside `sessionId`'s cwd (creating
 * parent dirs as needed) and returns the absolute path written. The backing
 * command rejects absolute paths and any `..` segment, and re-verifies the
 * resolved parent directory is still inside the session's cwd. Used by the
 * Plan tab's "Save to Workspace" menu item (`PlanToolbar`). */
export const saveTextFile = (
  sessionId: string,
  relativePath: string,
  content: string,
): Promise<string> =>
  invoke("save_text_file", { sessionId, relativePath, content })

/** Reads a UTF-8 text file relative to `sessionId`'s cwd for the Files
 * (Monaco) editor. Rejects absolute/`..` paths, binaries, non-UTF-8, and
 * files over ~1.5MB. */
export const readTextFile = (
  sessionId: string,
  relativePath: string,
): Promise<string> =>
  invoke("read_text_file", { sessionId, relativePath })

/** Creates an empty text file under `sessionId`'s cwd. Fails if it exists. */
export const createTextFile = (
  sessionId: string,
  relativePath: string,
): Promise<string> =>
  invoke("create_text_file", { sessionId, relativePath })

/** Renames a file under `sessionId`'s cwd. Both paths are repo-relative. */
export const renamePath = (
  sessionId: string,
  fromPath: string,
  toPath: string,
): Promise<string> =>
  invoke("rename_path", { sessionId, fromPath, toPath })

/** Deletes a file under `sessionId`'s cwd (files only, not directories). */
export const deletePath = (
  sessionId: string,
  relativePath: string,
): Promise<string> =>
  invoke("delete_path", { sessionId, relativePath })

/** Writes a diagnostics bundle (frontend payload + backend log tail) into
 * the app log directory — no active session required. */
export const exportDiagnosticsBundle = (
  frontendPayload: string,
): Promise<string> =>
  invoke("export_diagnostics_bundle", { frontendPayload })

/** Poll code-index status for a repo cwd (app-data index; no AgentEvent). */
export const indexStatus = (cwd: string): Promise<IndexStatus> =>
  invoke("index_status", { cwd })

/** Rebuild the code index for a repo cwd. */
export const indexRebuild = (cwd: string): Promise<IndexRebuildResult> =>
  invoke("index_rebuild", { cwd })

export const appVersion = (): Promise<string> => invoke("app_version")

/** Persists a pasted/dropped image blob's raw bytes to a uniquely-named file
 * in the OS temp dir and returns the absolute path — the only way to turn an
 * in-memory clipboard blob into a `PromptAttachment.path` the engine can read
 * (see `composerAttachments.ts::attachImageBlob`). `ext` is validated
 * server-side against an allowlist (png/jpg/jpeg/gif/webp) and `bytes` is
 * size-capped; see `write_temp_blob` in `src-tauri/src/commands.rs`. */
export const writeTempBlob = (bytes: Uint8Array, ext: string): Promise<string> =>
  invoke("write_temp_blob", { bytes: Array.from(bytes), ext })

/** Absolute path of the backend's rolling debug log file (see `init_tracing`
 * in `src-tauri/src/lib.rs`), for the Settings Diagnostics section's "Copy
 * log path"/"Open logs folder" affordance. */
export const debugLogPath = (): Promise<string> => invoke("debug_log_path")

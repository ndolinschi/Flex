import { invoke as tauriInvoke } from "@tauri-apps/api/core"
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event"
import {
  browserInvoke,
  browserListenBrowserState,
  browserListenSessionEvents,
  browserListenTerminalExit,
  browserListenTerminalOutput,
  isBrowserPreview,
} from "./browserMock"
import type {
  BrowserStateEvent,
  BuiltinProvider,
  CommandInfoDto,
  CreateSessionInput,
  FileHit,
  GitFileStatus,
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
} from "./types"

const invoke = <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
  if (isBrowserPreview()) return browserInvoke<T>(cmd, args)
  return tauriInvoke<T>(cmd, args)
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

export const respondPermission = (input: RespondPermissionInput): Promise<void> =>
  invoke("respond_permission", { input })

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

export const gitBranch = (cwd: string): Promise<string | null> =>
  invoke("git_branch", { cwd })

export const gitListBranches = (cwd: string): Promise<string[]> =>
  invoke("git_list_branches", { cwd })

export const gitCheckout = (cwd: string, branch: string): Promise<void> =>
  invoke("git_checkout", { cwd, branch })

export const gitStatus = (cwd: string): Promise<GitFileStatus[]> =>
  invoke("git_status", { cwd })

/** Files changed since the session's baseline (captured at create/resume).
 * Gracefully degrades to full `git_status` for isolated sessions, missing
 * baselines, or a moved HEAD. */
export const gitStatusSinceBaseline = (
  sessionId: string,
): Promise<GitFileStatus[]> =>
  invoke("git_status_since_baseline", { sessionId })

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

export const listFiles = (cwd: string, query: string): Promise<FileHit[]> =>
  invoke("list_files", { cwd, query })

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
  if (isBrowserPreview()) return browserListenSessionEvents(handler)
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

export const listenTerminalOutput = (
  handler: (event: TerminalOutputEvent) => void,
): Promise<UnlistenFn> => {
  if (isBrowserPreview()) return browserListenTerminalOutput(handler)
  return tauriListen<TerminalOutputEvent>("terminal-output", (e) => {
    handler(e.payload)
  })
}

export const listenTerminalExit = (
  handler: (event: TerminalExitEvent) => void,
): Promise<UnlistenFn> => {
  if (isBrowserPreview()) return browserListenTerminalExit(handler)
  return tauriListen<TerminalExitEvent>("terminal-exit", (e) => {
    handler(e.payload)
  })
}

export const listenBrowserState = (
  handler: (event: BrowserStateEvent) => void,
): Promise<UnlistenFn> => {
  if (isBrowserPreview()) return browserListenBrowserState(handler)
  return tauriListen<BrowserStateEvent>("browser-state", (e) => {
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

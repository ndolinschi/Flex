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
  InlineCompletionPrefs,
  CheckInlineCompletionResult,
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

export const setSecretStorage = (
  mode: SecretStorageMode,
): Promise<ProviderConfigView> => invoke("set_secret_storage", { mode })

export const platformType = async (): Promise<string> => {
  if (isBrowserPreview()) throw new Error(NATIVE_APP_REQUIRED)
  const { type } = await import("@tauri-apps/plugin-os")
  return type()
}

export const profilesList = (): Promise<ProviderProfileView[]> =>
  invoke("profiles_list")

export const profileUpsert = (
  profile: ProviderProfileInput,
): Promise<ProviderProfileView> => invoke("profile_upsert", { profile })

export const profileRemove = (id: string): Promise<void> =>
  invoke("profile_remove", { id })

export const profileActivate = (id: string): Promise<ProviderConfigView> =>
  invoke("profile_activate", { id })

export const validateProfile = (
  input: ProviderProfileInput,
): Promise<ModelInfoDto[]> => invoke("validate_profile", { input })

export const copilotAuthStatus = (): Promise<CopilotAuthStatus> =>
  invoke("copilot_auth_status")

export const copilotAuthStart = (): Promise<CopilotAuthStart> =>
  invoke("copilot_auth_start")

export const copilotAuthWait = (
  sessionId: string,
): Promise<CopilotAuthStatus> => invoke("copilot_auth_wait", { sessionId })

export const copilotAuthCancel = (sessionId: string): Promise<void> =>
  invoke("copilot_auth_cancel", { sessionId })

export const chatgptAuthStatus = (): Promise<ChatgptAuthStatus> =>
  invoke("chatgpt_auth_status")

export const chatgptAuthStart = (): Promise<ChatgptAuthStart> =>
  invoke("chatgpt_auth_start")

export const chatgptAuthWait = (
  sessionId: string,
): Promise<ChatgptAuthStatus> => invoke("chatgpt_auth_wait", { sessionId })

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

export const suggestSessionTitle = (
  sessionId: string,
  promptText: string,
): Promise<string> =>
  invoke("suggest_session_title", { sessionId, promptText })

export const getInlineCompletionPrefs = (): Promise<InlineCompletionPrefs> =>
  invoke("get_inline_completion_prefs")

export const saveInlineCompletionPrefs = (
  prefs: InlineCompletionPrefs,
): Promise<InlineCompletionPrefs> =>
  invoke("save_inline_completion_prefs", { prefs })

export const INLINE_COMPLETION_NOT_CONFIGURED = "inline_completion_not_configured"

export const completePromptInline = (
  prefix: string,
  suffix?: string,
): Promise<string> =>
  invoke("complete_prompt_inline", {
    prefix,
    suffix: suffix ?? null,
  })

export const checkInlineCompletionConnection = (
  providerId: string,
  modelId: string,
): Promise<CheckInlineCompletionResult> =>
  invoke("check_inline_completion_connection", {
    input: { providerId, modelId },
  })

export type PromptReviewFinding = {
  quote: string
  severity: "error" | "warn" | "info" | string
  message: string
  fix?: string | null
}

export type PromptReview = {
  summary: string
  findings: PromptReviewFinding[]
  questions?: string[]
}

export type PromptReviewAnswer = {
  question: string
  answer: string
}

export const reviewPrompt = (
  sessionId: string,
  promptText: string,
  answers?: PromptReviewAnswer[],
): Promise<PromptReview> =>
  invoke("review_prompt", {
    sessionId,
    promptText,
    answers: answers?.length ? answers : null,
  })

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

export const backgroundList = (
  sessionId: string,
): Promise<BackgroundProcessDto[]> => invoke("background_list", { sessionId })

export const backgroundKill = (
  sessionId: string,
  processId: string,
): Promise<void> => invoke("background_kill", { sessionId, processId })

export const backgroundDemote = (
  sessionId: string,
  callId: string,
): Promise<boolean> => invoke("background_demote", { sessionId, callId })

export const respondPermission = (input: RespondPermissionInput): Promise<void> =>
  invoke("respond_permission", { input })

export const setTurnPermissionMode = (
  sessionId: string,
  mode: string | null,
): Promise<void> => invoke("set_turn_permission_mode", { sessionId, mode })

export const respondQuestion = (input: RespondQuestionInput): Promise<void> =>
  invoke("respond_question", { input })

export const respondModeSwitch = (
  sessionId: string,
  id: string,
  allow: boolean,
): Promise<void> => invoke("respond_mode_switch", { input: { sessionId, id, allow } })

export const isConfigured = (): Promise<boolean> => invoke("is_configured")

export const gitIsRepo = (cwd: string): Promise<boolean> =>
  invoke("git_is_repo", { cwd })

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

export const gitStatusSinceBaseline = (
  sessionId: string,
): Promise<GitStatusSummary> => invoke("git_status_since_baseline", { sessionId })

export type GitStatusBatchEntry = {
  sessionId: string
  summary?: GitStatusSummary
  error?: string
}

/** Single IPC for many sessions (sidebar badges). Caps at 64 server-side. */
export const gitStatusSinceBaselineBatch = (
  sessionIds: string[],
): Promise<GitStatusBatchEntry[]> =>
  invoke("git_status_since_baseline_batch", { sessionIds })

export const gitDiff = (cwd: string, path: string): Promise<string> =>
  invoke("git_diff", { cwd, path })

export const gitCommit = (
  sessionId: string,
  message: string,
): Promise<string> => invoke("git_commit", { sessionId, message })

export const gitPush = (sessionId: string): Promise<void> =>
  invoke("git_push", { sessionId })

export const gitCommitPaths = (
  sessionId: string,
  message: string,
  paths: string[],
): Promise<string> =>
  invoke("git_commit_paths", { sessionId, message, paths })

export const gitCommitAndPush = (
  sessionId: string,
  message: string,
  paths: string[],
): Promise<string> =>
  invoke("git_commit_and_push", { sessionId, message, paths })

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
  degradedReason: string | null
}

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

export const gitPrStatus = (cwd: string): Promise<BranchPrStatus> =>
  invoke("git_pr_status", { cwd })

/** Paths changed in the open PR (for paged review). */
export const gitPrFiles = (cwd: string): Promise<string[]> =>
  invoke("git_pr_files", { cwd })

/** Full PR diff, or a single path when `path` is set. */
export const gitPrDiff = (cwd: string, path?: string): Promise<string> =>
  invoke("git_pr_diff", { cwd, path: path ?? null })

export type PrDraft = {
  title: string
  body: string
}

export const gitPrDraft = (cwd: string): Promise<PrDraft> =>
  invoke("git_pr_draft", { cwd })

export const gitCreatePrForBranch = (
  cwd: string,
  title?: string,
  body?: string,
): Promise<CreatePrOutcome> =>
  invoke("git_create_pr_for_branch", { cwd, title, body })

export const suggestCommitMessage = (
  sessionId: string,
  diffSummary: string,
): Promise<string> =>
  invoke("suggest_commit_message", { sessionId, diffSummary })

export const listFiles = (
  cwd: string,
  query: string,
  includeIgnored = false,
  fallbackCwd?: string | null,
): Promise<FileHit[]> =>
  invoke("list_files", {
    cwd,
    query,
    includeIgnored,
    fallbackCwd: fallbackCwd || null,
  })

export const listDirChildren = (
  cwd: string,
  relativeDir: string,
  fallbackCwd?: string | null,
): Promise<FileHit[]> =>
  invoke("list_dir_children", {
    cwd,
    relativeDir,
    fallbackCwd: fallbackCwd || null,
  })

export const invalidateWorkspacePathCache = (
  cwd?: string | null,
): Promise<void> =>
  invoke("invalidate_workspace_path_cache", { cwd: cwd || null })

export const resolveWorkspaceCwd = (
  cwd: string,
  fallbackCwd?: string | null,
): Promise<string | null> =>
  invoke("resolve_workspace_cwd", {
    cwd,
    fallbackCwd: fallbackCwd || null,
  })

export type DbEngine = "sqlite" | "postgres" | "mysql"

export type DbConnectionSpec = {
  id: string
  name: string
  engine: DbEngine
  target: string
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

export type ComponentsDetectResult = {
  isReact: boolean
  frameworks?: string[]
  reason: string
  packageName: string | null
}

export type ComponentNode = {
  id: string
  name: string
  file: string
  exportName: string
  children: string[]
}

export type ComponentsListResult = {
  isReact: boolean
  frameworks?: string[]
  components: ComponentNode[]
  roots: string[]
}

export type ComponentPropSummary = {
  name: string
  optional: boolean
  typeHint: string | null
}

export type ComponentDetail = {
  id: string
  name: string
  file: string
  exportName: string
  props: ComponentPropSummary[]
  sourceSnippet: string
  children: string[]
}

export const componentsDetect = (
  cwd: string,
  fallbackCwd?: string | null,
): Promise<ComponentsDetectResult> =>
  invoke("components_detect", { cwd, fallbackCwd: fallbackCwd || null })

export const componentsList = (
  cwd: string,
  fallbackCwd?: string | null,
): Promise<ComponentsListResult> =>
  invoke("components_list", { cwd, fallbackCwd: fallbackCwd || null })

export const componentsDetail = (
  cwd: string,
  id: string,
  fallbackCwd?: string | null,
): Promise<ComponentDetail> =>
  invoke("components_detail", { cwd, id, fallbackCwd: fallbackCwd || null })

export const browserApplyStyleOverrides = (
  selector: string,
  styles: Record<string, string>,
): Promise<void> => invoke("browser_apply_style_overrides", { selector, styles })

export const isIsolated = (sessionId: string): Promise<boolean> =>
  invoke("is_isolated", { sessionId })

export const workspaceStatus = (
  sessionId: string,
): Promise<WorkspaceStatusDto | null> =>
  invoke("workspace_status", { sessionId })

export type WorkspaceInfo = {
  id: string
  path: string
  baseRef: string
}

export const listWorkspaces = (cwd: string): Promise<WorkspaceInfo[]> =>
  invoke("list_workspaces", { cwd })

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

export const listenSessionBaselineReady = (
  handler: (payload: { sessionId: string }) => void,
): Promise<UnlistenFn> => {
  if (isBrowserPreview()) return Promise.resolve(() => {})
  return tauriListen<{ sessionId: string }>("session-baseline-ready", (e) => {
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

export const mcpTest = (id: string): Promise<string[]> =>
  invoke("mcp_test", { id })

export const memoryList = (): Promise<MemoryEntryDto[]> => invoke("memory_list")

export const memoryGet = (id: string): Promise<MemoryEntryDto> =>
  invoke("memory_get", { id })

export const memoryRemove = (id: string): Promise<void> =>
  invoke("memory_remove", { id })

export const memorySetExpiry = (
  id: string,
  expiresAtMs: number | undefined,
): Promise<void> => invoke("memory_set_expiry", { id, expiresAtMs })

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

export const browserOpenDevtools = (): Promise<void> =>
  invoke("browser_open_devtools")

export const browserHardReload = (): Promise<void> =>
  invoke("browser_hard_reload")

export const browserClearData = (): Promise<void> => invoke("browser_clear_data")

export const browserScreenshot = (): Promise<string> => invoke("browser_screenshot")

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

export const reviewUndoFile = (
  sessionId: string,
  path: string,
): Promise<void> => invoke("review_undo_file", { sessionId, path })

export const reviewKeepFile = (
  sessionId: string,
  path: string,
): Promise<void> => invoke("review_keep_file", { sessionId, path })

export const reviewApplyPatch = (
  sessionId: string,
  patch: string,
  target: ReviewPatchTarget,
  reverse: boolean,
): Promise<void> =>
  invoke("review_apply_patch", { sessionId, patch, target, reverse })

export const reviewFileDiff = (
  sessionId: string,
  path: string,
): Promise<string> => invoke("review_file_diff", { sessionId, path })

export type ArtifactKind =
  | "presentation"
  | "spreadsheet"
  | "csv"
  | "diagram"
  | "image"
  | "document"
  | "other"

export type Artifact = {
  id: string
  projectKey: string
  sessionId: string
  kind: ArtifactKind
  relativePath: string
  title: string
  createdAt: string
  mimeType?: string
}

export type CsvPreview = {
  columns: string[]
  rows: string[][]
  truncated: boolean
  rowCount: number
}

export const artifactsList = (projectKey: string): Promise<Artifact[]> =>
  invoke("artifacts_list", { projectKey })

export const artifactsRegister = (
  projectKey: string,
  sessionId: string,
  relativePath: string,
  title?: string,
): Promise<Artifact> =>
  invoke("artifacts_register", {
    projectKey,
    sessionId,
    relativePath,
    title: title ?? null,
  })

export const artifactsRemove = (projectKey: string, id: string): Promise<void> =>
  invoke("artifacts_remove", { projectKey, id })

export const artifactsPreviewCsv = (
  projectKey: string,
  id: string,
  maxRows?: number,
): Promise<CsvPreview> =>
  invoke("artifacts_preview_csv", { projectKey, id, maxRows: maxRows ?? null })

export const artifactsOpenExternal = (
  projectKey: string,
  id: string,
): Promise<void> => invoke("artifacts_open_external", { projectKey, id })

export const toInvokeError = (err: unknown): string => {
  if (typeof err === "string") return err
  if (err instanceof Error) return err.message
  return "An unexpected error occurred"
}

/** Coarse classification for UI branching (retry vs empty vs reauth). */
export type InvokeErrorKind =
  | "session_not_found"
  | "not_configured"
  | "permission"
  | "not_found"
  | "network"
  | "unknown"

export const classifyInvokeError = (err: unknown): InvokeErrorKind => {
  const msg = toInvokeError(err).toLowerCase()
  if (/session\s+\S+\s+not found/.test(msg) || /session not found/.test(msg)) {
    return "session_not_found"
  }
  if (/not configured|save a provider|no provider/.test(msg)) {
    return "not_configured"
  }
  if (/permission|denied|not allowed|eacces|eperm/.test(msg)) {
    return "permission"
  }
  if (/\bnot found\b|enoent|no such file/.test(msg)) {
    return "not_found"
  }
  if (/network|econnrefused|etimedout|fetch failed|offline|dns/.test(msg)) {
    return "network"
  }
  return "unknown"
}

export type UserIdentityDto = {
  name: string
}

export const userIdentity = (): Promise<UserIdentityDto> =>
  invoke("user_identity")

export const saveTextFile = (
  sessionId: string,
  relativePath: string,
  content: string,
): Promise<string> =>
  invoke("save_text_file", { sessionId, relativePath, content })

export const readTextFile = (
  sessionId: string,
  relativePath: string,
): Promise<string> =>
  invoke("read_text_file", { sessionId, relativePath })

export const createTextFile = (
  sessionId: string,
  relativePath: string,
): Promise<string> =>
  invoke("create_text_file", { sessionId, relativePath })

export const renamePath = (
  sessionId: string,
  fromPath: string,
  toPath: string,
): Promise<string> =>
  invoke("rename_path", { sessionId, fromPath, toPath })

export const deletePath = (
  sessionId: string,
  relativePath: string,
): Promise<string> =>
  invoke("delete_path", { sessionId, relativePath })

export const exportDiagnosticsBundle = (
  frontendPayload: string,
): Promise<string> =>
  invoke("export_diagnostics_bundle", { frontendPayload })

export const indexStatus = (cwd: string): Promise<IndexStatus> =>
  invoke("index_status", { cwd })

export const indexRebuild = (cwd: string): Promise<IndexRebuildResult> =>
  invoke("index_rebuild", { cwd })

export const appVersion = (): Promise<string> => invoke("app_version")

export type CloudflarePrefs = {
  enabled: boolean
  hostname?: string | null
}

export type MethodPrefs = {
  manual: boolean
  lan: boolean
  bonjour: boolean
  publicPort: boolean
  cloudflare: CloudflarePrefs
  bluetooth: boolean
}

export type RemoteAccessConfig = {
  enabled: boolean
  deviceName: string
  deviceId: string
  port: number
  methods: MethodPrefs
}

export type PairingEndpoint = {
  method: string
  url?: string | null
  host?: string | null
  port?: number | null
  service_type?: string | null
  tunnel_hostname?: string | null
  status?: string | null
  note?: string | null
}

export type PairingInfo = {
  protocol_version: number
  app_version: string
  device_name: string
  device_id: string
  auth: { type: string; token?: string | null }
  endpoints: PairingEndpoint[]
  capabilities: string[]
  openapi_url: string
}

export type MethodNote = {
  id: string
  status: string
  note?: string | null
}

export type RemoteAccessStatus = {
  config: RemoteAccessConfig
  running: boolean
  bindAddr?: string | null
  token?: string | null
  pairing?: PairingInfo | null
  pairingJson?: string | null
  pairingQrSvg?: string | null
  methodNotes: MethodNote[]
}

export type SaveRemoteAccessInput = {
  enabled: boolean
  deviceName?: string | null
  port?: number | null
  methods?: MethodPrefs | null
}

export const remoteAccessGet = (): Promise<RemoteAccessStatus> =>
  invoke("remote_access_get")

export const remoteAccessSave = (
  input: SaveRemoteAccessInput,
): Promise<RemoteAccessStatus> => invoke("remote_access_save", { input })

export const remoteAccessRotateToken = (): Promise<RemoteAccessStatus> =>
  invoke("remote_access_rotate_token")

export const remoteAccessRestart = (): Promise<RemoteAccessStatus> =>
  invoke("remote_access_restart")

export const writeTempBlob = (bytes: Uint8Array, ext: string): Promise<string> =>
  invoke("write_temp_blob", { bytes: Array.from(bytes), ext })

export const debugLogPath = (): Promise<string> => invoke("debug_log_path")

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
  ModelInfoDto,
  PromptCommandInput,
  ProviderConfigView,
  RespondPermissionInput,
  RespondQuestionInput,
  RoutineDto,
  RoutineRunRecordDto,
  SaveProviderConfigInput,
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

export const gitBranch = (cwd: string): Promise<string | null> =>
  invoke("git_branch", { cwd })

export const gitListBranches = (cwd: string): Promise<string[]> =>
  invoke("git_list_branches", { cwd })

export const gitCheckout = (cwd: string, branch: string): Promise<void> =>
  invoke("git_checkout", { cwd, branch })

export const gitStatus = (cwd: string): Promise<GitFileStatus[]> =>
  invoke("git_status", { cwd })

export const gitDiff = (cwd: string, path: string): Promise<string> =>
  invoke("git_diff", { cwd, path })

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

export const toInvokeError = (err: unknown): string => {
  if (typeof err === "string") return err
  if (err instanceof Error) return err.message
  return "An unexpected error occurred"
}

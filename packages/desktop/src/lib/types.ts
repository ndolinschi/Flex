// Wire types hand-synced with engine contracts + desktop Tauri commands.

export type SessionId = string
export type TurnId = string
export type MessageId = string
export type ToolCallId = string
export type PermissionRequestId = string

export type Role = "user" | "assistant" | "system"

export type IsolationPolicy = "never" | "optional" | "required"

export type SessionMeta = {
  id: SessionId
  title?: string
  agent_id: string
  parent_id?: SessionId
  role?: string
  depth: number
  provider_session_id?: string
  cwd: string
  model?: string
  fallback_models: string[]
  mode?: string
  isolation?: IsolationPolicy
  workspace_id?: string
  executor?: string
  base_cwd?: string
  created_at_ms: number
  updated_at_ms: number
}

export type TokenUsage = {
  input: number
  output: number
  cache_read?: number
  cache_write?: number
  reasoning?: number
}

export type TurnStopReason =
  | "end_turn"
  | "max_tokens"
  | "max_iterations"
  | "refusal"
  | "cancelled"
  | "error"

export type TurnSummary = {
  turn_id: TurnId
  stop_reason: TurnStopReason
  usage: TokenUsage
  cost_usd?: number
  num_model_calls: number
  num_tool_calls: number
  duration_ms: number
}

export type ErrorCode =
  | "auth_missing"
  | "auth_expired"
  | "rate_limited"
  | "model_unavailable"
  | "permission_denied"
  | "cancelled"
  | "process_crashed"
  | "protocol_violation"
  | "timeout"
  | "not_installed"
  | "invalid_request"
  | "context_overflow"
  | "unknown"

export type Provenance =
  | { from: "native"; provider: string }
  | { from: "delegator"; agent_id: string; exit_code?: number; stderr_tail?: string }
  | { from: "engine" }

export type EngineError = {
  code: ErrorCode
  message: string
  retryable: boolean
  provenance: Provenance
  retry_after_ms?: number
  detail?: unknown
}

export type BlobSource =
  | { source: "base64"; data: string }
  | { source: "url"; url: string }
  | { source: "path"; path: string }

export type ContentBlock =
  | { type: "markdown"; text: string }
  | { type: "image"; media_type: string; data: BlobSource }
  | { type: "file"; name: string; media_type: string; data: BlobSource }
  | { type: "thinking"; text: string; signature?: string }
  | { type: "tool_use"; id: ToolCallId; name: string; input: unknown }
  | { type: "tool_result"; tool_use_id: ToolCallId; content: ToolResultBlock[]; is_error?: boolean }
  | { type: "unknown"; raw: unknown }

export type ToolResultBlock =
  | { type: "markdown"; text: string }
  | { type: "image"; media_type: string; data: BlobSource }
  | { type: "json"; value: unknown }

export type PermissionDecisionKind = "allow_once" | "allow_always" | "deny"

export type ToolCallOrigin =
  | { origin: "model" }
  | { origin: "hook" }
  | { origin: "subagent" }
  | { origin: "external"; agent_id: string }

export type ToolCallStatus =
  | { state: "pending" }
  | { state: "awaiting_permission"; request_id: PermissionRequestId }
  | { state: "running" }
  | { state: "completed" }
  | { state: "failed"; error: string }
  | { state: "denied"; reason?: string }
  | { state: "cancelled" }

export type ToolCallTiming = {
  queued_at_ms: number
  permission_wait_ms?: number
  started_at_ms?: number
  finished_at_ms?: number
}

export type ToolOutput = {
  content: ToolResultBlock[]
  is_error: boolean
  structured?: unknown
}

export type ToolCall = {
  id: ToolCallId
  session_id: SessionId
  turn_id: TurnId
  message_id: MessageId
  tool_name: string
  input: unknown
  read_only: boolean
  origin: ToolCallOrigin
  status: ToolCallStatus
  timing: ToolCallTiming
  result?: ToolOutput
}

export type PlanStatus = "pending" | "in_progress" | "completed"

export type PlanEntry = {
  content: string
  status: PlanStatus
}

export type QuestionOption = {
  label: string
  description?: string
}

export type Question = {
  header: string
  question: string
  options: QuestionOption[]
  multi_select?: boolean
  allow_custom?: boolean
}

export type Answer = {
  question: string
  selected: string[]
}

export type AgentEvent =
  | { kind: "session_created"; meta: SessionMeta }
  | { kind: "engine_info"; agent_id: string; capabilities: unknown; provider_session_id?: string; resolution_trace?: string[] }
  | { kind: "turn_started"; turn_id: TurnId }
  | { kind: "turn_completed"; turn_id: TurnId; summary: TurnSummary }
  | { kind: "session_error"; error: EngineError }
  | { kind: "message_started"; message_id: MessageId; role: Role }
  | { kind: "markdown_delta"; message_id: MessageId; text: string }
  | { kind: "thinking_delta"; message_id: MessageId; text: string }
  | { kind: "text_snapshot"; message_id: MessageId; text: string }
  | { kind: "tool_args_delta"; call_id: ToolCallId; json_fragment: string }
  | { kind: "tool_progress"; call_id: ToolCallId; note: string }
  | { kind: "user_message"; message_id: MessageId; content: ContentBlock[] }
  | { kind: "assistant_message"; message_id: MessageId; content: ContentBlock[]; model?: string; usage?: TokenUsage }
  | { kind: "tool_call_updated"; call: ToolCall }
  | { kind: "plan_updated"; entries: PlanEntry[] }
  | { kind: "permission_requested"; id: PermissionRequestId; call_id?: ToolCallId; title: string; detail?: string; options: PermissionDecisionKind[] }
  | { kind: "permission_resolved"; id: PermissionRequestId; decision: unknown }
  | { kind: "question_requested"; id: string; questions: Question[] }
  | { kind: "question_resolved"; id: string; answers: Answer[] }
  | { kind: "command_expanded"; name: string; args: string }
  | { kind: "compaction_boundary"; summary: unknown }
  | { kind: "model_fallback"; from: string; to?: string; reason: EngineError }
  | { kind: "hook_fired"; point: string; outcome: string }
  | { kind: "subagent_started"; child_session: SessionId; task: string; call_id?: ToolCallId; role?: string }
  | { kind: "subagent_event"; child_session: SessionId; event: AgentEvent }
  | { kind: "subagent_completed"; child_session: SessionId; summary: TurnSummary }
  | { kind: "workspace_provisioned"; workspace_id: string; path: string; base_ref: string }
  | { kind: "workspace_integrated"; workspace_id: string; outcome: unknown }
  | { kind: "workspace_discarded"; workspace_id: string }
  | { kind: "snapshot_created"; snapshot_id: string; turn_id: TurnId }
  | { kind: "snapshot_restored"; snapshot_id: string }
  | { kind: "gap"; from_seq: number }
  | { kind: "unknown"; raw: unknown }

export type SessionEvent = {
  session_id: SessionId
  seq: number
  turn_id?: TurnId
  ts_ms: number
  payload: AgentEvent
}

// Desktop command DTOs (camelCase serde)

export type PluginPrefs = {
  search: boolean
  learning: boolean
  verifier: boolean
}

export type ProviderConfigView = {
  preferredProvider?: string
  baseUrl?: string
  defaultModel?: string
  cwd?: string
  configuredProviders: string[]
  hasAnyKey: boolean
  plugins: PluginPrefs
  fallbackModels: string[]
  defaultIsolation?: IsolationPolicy | string
}

export type SaveProviderConfigInput = {
  preferredProvider: string
  apiKey?: string
  baseUrl?: string
  defaultModel?: string
  cwd?: string
  plugins?: PluginPrefs
  fallbackModels?: string[]
  defaultIsolation?: IsolationPolicy | string
}

export type BuiltinProvider = {
  id: string
  label: string
  requiresApiKey: boolean
}

export type ModelInfoDto = {
  id: string
  displayName?: string
  providerId: string
  contextWindow?: number
}

export type CreateSessionInput = {
  title?: string
  model?: string
  cwd?: string
  isolation?: IsolationPolicy
}

export type UpdateSessionInput = {
  title?: string
  model?: string
  cwd?: string
}

export type CommandInfoDto = {
  name: string
  description: string
  argsHint?: string
}

export type WorkspaceStatusDto = {
  filesChanged: number
  summary: string
}

/** One changed file from `git_status` (Changes panel). */
export type GitFileStatus = {
  /** Path relative to the session cwd. */
  path: string
  /** Porcelain letter: "M" | "A" | "D" | "R" | "?" (untracked). */
  status: string
  added?: number
  removed?: number
}

/** A file match from `list_files`, used by composer @-mentions. */
export type FileHit = {
  /** Path relative to the session cwd, forward-slashed. */
  path: string
  /** Basename, shown as the primary label. */
  name: string
}

export type PromptAttachment = {
  path: string
  kind: "image" | "file"
  name?: string
  mediaType?: string
}

export type ComposerMode = "agent" | "plan" | "ask"

export type PermissionMode =
  | "default"
  | "accept_edits"
  | "plan"
  | "dont_ask"
  | "bypass_permissions"

export type PromptCommandInput = {
  sessionId: string
  text: string
  model?: string
  permissionMode?: PermissionMode
  attachments?: PromptAttachment[]
}

export type ComposerAttachment = {
  id: string
  path: string
  kind: "image" | "file"
  name: string
}

export type RespondPermissionInput = {
  sessionId: string
  requestId: string
  decision: string
  reason?: string
}

export type AppRoute = "chat" | "settings" | "customize" | "automations" | "welcome"

export type RoutineTriggerDto = {
  kind: "cron" | "webhook"
  expr?: string
  path?: string
}

export type RoutineDto = {
  id: string
  prompt: string
  maxIterations: number
  maxIdenticalFailures: number
  tokenBudget?: number
  requireVerification: boolean
  trigger: RoutineTriggerDto
  title?: string
  cwd?: string
  model?: string
}

export type RoutineRunRecordDto = {
  sessionId: string
  startedMs: number
  stopReason: string
  iterations: number
}

export type StreamingBuffers = {
  markdown: Record<MessageId, string>
  thinking: Record<MessageId, string>
  toolCalls: Record<ToolCallId, ToolCall>
  /** Latest progress note per running tool call (from `tool_progress`). */
  toolProgress: Record<ToolCallId, string>
  /** Accumulated partial input JSON per running tool call (from `tool_args_delta`). */
  toolArgs: Record<ToolCallId, string>
}

export type PendingPermission = {
  sessionId: SessionId
  requestId: PermissionRequestId
  title: string
  detail?: string
  options: PermissionDecisionKind[]
  callId?: ToolCallId
}

export type PendingQuestion = {
  sessionId: SessionId
  requestId: string
  questions: Question[]
}

export type RespondQuestionInput = {
  sessionId: string
  requestId: string
  answers: Answer[]
}

export type TimelineRow =
  | { type: "user"; id: string; messageId: MessageId; text: string; tsMs: number }
  | { type: "assistant"; id: string; messageId: MessageId; text: string; model?: string; tsMs: number }
  | { type: "thinking"; id: string; messageId: MessageId; text: string; tsMs: number }
  | { type: "tool"; id: string; call: ToolCall; tsMs: number }
  | { type: "plan"; id: string; entries: PlanEntry[]; tsMs: number }
  | { type: "turn"; id: string; turnId: TurnId; phase: "started" | "completed"; summary?: TurnSummary; tsMs: number }
  | { type: "error"; id: string; error: EngineError; tsMs: number }
  | { type: "fallback"; id: string; from: string; to?: string; reason: string; tsMs: number }
  | { type: "command"; id: string; name: string; args: string; tsMs: number }
  | { type: "meta"; id: string; text: string; tsMs: number }
  | {
      type: "subagent"
      id: string
      childSession: SessionId
      task: string
      role?: string
      phase: "started" | "completed"
      summary?: TurnSummary
      children: TimelineRow[]
      tsMs: number
    }

export const extractMarkdownText = (blocks: ContentBlock[]): string => {
  const parts: string[] = []
  for (const block of blocks) {
    if (block.type === "markdown") {
      parts.push(block.text)
    }
  }
  return parts.join("\n\n")
}

export const extractThinkingText = (blocks: ContentBlock[]): string => {
  const parts: string[] = []
  for (const block of blocks) {
    if (block.type === "thinking") {
      parts.push(block.text)
    }
  }
  return parts.join("\n\n")
}

/** True when a user_message should render as a chat bubble (not tool-result feedback). */
export const hasVisibleUserContent = (blocks: ContentBlock[]): boolean => {
  for (const block of blocks) {
    if (block.type === "markdown" && block.text.trim()) return true
    if (block.type === "image" || block.type === "file") return true
  }
  return false
}

export const DEFAULT_SESSION_TITLE = "New Agent"

export const truncateId = (id: string, len = 8): string => {
  if (id.length <= len) return id
  return `${id.slice(0, len)}…`
}

/** True when the session still has the placeholder title (or none). */
export const isDefaultSessionTitle = (title?: string | null): boolean => {
  const t = title?.trim()
  return !t || t === DEFAULT_SESSION_TITLE
}

/** Title derived from the first user prompt (Cursor-style). */
export const titleFromPrompt = (text: string, maxLen = 48): string => {
  const cleaned = text.replace(/\s+/g, " ").trim()
  if (!cleaned) return DEFAULT_SESSION_TITLE
  if (cleaned.length <= maxLen) return cleaned
  const slice = cleaned.slice(0, maxLen)
  const lastSpace = slice.lastIndexOf(" ")
  const base = lastSpace > 16 ? slice.slice(0, lastSpace) : slice
  return `${base.trimEnd()}…`
}

export const sessionLabel = (meta: SessionMeta): string => {
  if (meta.title?.trim()) return meta.title.trim()
  return DEFAULT_SESSION_TITLE
}

// Right-panel Terminal + Browser features (desktop-only, camelCase serde)

export type TerminalInfo = { id: string; cwd: string; createdAtMs: number }

export type TerminalOutputEvent = { id: string; data: string }

export type TerminalExitEvent = { id: string; exitCode?: number }

export type BrowserStateEvent = {
  url: string
  title: string | null
  loading: boolean
  canGoBack: boolean
  canGoForward: boolean
}

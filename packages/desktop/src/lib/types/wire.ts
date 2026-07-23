
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

export type CompactionSummary = {
  summary_markdown: string
  strategy: string
  tokens_before?: number
  tokens_after?: number
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

export type BackgroundProcessDto = {
  process_id: string
  command?: string | null
  running: boolean
  started_at_ms?: number | null
  exit_code?: number | null
}

export type PlanStatus = "pending" | "in_progress" | "completed"

export type PlanEntry = {
  content: string
  status: PlanStatus
}

export type VerdictOutcome = "pass" | "fail" | "inconclusive"

export type VerificationVerdict = {
  outcome: VerdictOutcome
  findings: string[]
  confidence?: number
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

export type ExecStream = "stdout" | "stderr"

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
  | { kind: "exec_chunk"; call_id: ToolCallId; stream: ExecStream; text: string }
  | { kind: "user_message"; message_id: MessageId; content: ContentBlock[] }
  | { kind: "assistant_message"; message_id: MessageId; content: ContentBlock[]; model?: string; usage?: TokenUsage }
  | { kind: "tool_call_updated"; call: ToolCall }
  | { kind: "plan_updated"; entries: PlanEntry[] }
  | { kind: "permission_requested"; id: PermissionRequestId; call_id?: ToolCallId; title: string; detail?: string; options: PermissionDecisionKind[] }
  | { kind: "permission_resolved"; id: PermissionRequestId; decision: unknown }
  | { kind: "question_requested"; id: string; questions: Question[] }
  | { kind: "question_resolved"; id: string; answers: Answer[] }
  | { kind: "command_expanded"; name: string; args: string }
  | { kind: "compaction_boundary"; summary: CompactionSummary }
  | { kind: "compaction_started"; strategy: string }
  | { kind: "indexing_started"; reason: string }
  | { kind: "indexing_completed"; added: number; changed: number; removed: number; unchanged: number }
  | { kind: "model_fallback"; from: string; to?: string; reason: EngineError }
  | { kind: "retry_scheduled"; attempt: number; max_attempts: number; delay_ms: number; error: string }
  | { kind: "hook_fired"; point: string; outcome: string }
  | { kind: "subagent_started"; child_session: SessionId; task: string; call_id?: ToolCallId; role?: string }
  | { kind: "subagent_event"; child_session: SessionId; event: AgentEvent }
  | { kind: "subagent_completed"; child_session: SessionId; summary: TurnSummary }
  | { kind: "workspace_provisioned"; workspace_id: string; path: string; base_ref: string }
  | { kind: "workspace_integrated"; workspace_id: string; outcome: unknown }
  | { kind: "workspace_discarded"; workspace_id: string }
  | { kind: "snapshot_created"; snapshot_id: string; turn_id: TurnId }
  | { kind: "snapshot_restored"; snapshot_id: string }
  | { kind: "peer_message"; id: string; from: SessionId; to?: SessionId; thread_id?: string; content: string; about_path?: string }
  | { kind: "mode_switch_proposed"; id: string; mode: string; reason: string; timeout_ms: number }
  | { kind: "mode_switch_applied"; id: string; mode: string }
  | { kind: "mode_switch_rejected"; id: string; mode: string; reason?: string }
  | { kind: "routing_changed"; model?: string; effort?: string; reason: string }
  | { kind: "gap"; from_seq: number }
  | { kind: "unknown"; raw: unknown }

export type SessionEvent = {
  session_id: SessionId
  seq: number
  turn_id?: TurnId
  ts_ms: number
  payload: AgentEvent
}

export type PluginPrefs = {
  search: boolean
  index: boolean
  autoContext: boolean
  autoUpdateIndex: boolean
  learning: boolean
  learningRequireHumanApproval: boolean
  learningRequireVerifiedMemory: boolean
  verifier: boolean
  artifacts: boolean
  browser: boolean
  computer: boolean

  messaging: boolean
  council: boolean
  autoMode: boolean
  autoModeRouterModel?: string
  autoCompact: boolean
  autoCompactThresholdPercent: number
  compactionMode: string
  modeSwitchVetoMs: number
  delegationRules: string

  costMode: string
  costModelsLow: string[]
  costModelsMedium: string[]
  costModelsHigh: string[]
}

export type InlineCompletionPrefs = {
  enabled: boolean
  providerId?: string
  modelId?: string
  setupDismissed?: boolean
}

export type CheckInlineCompletionResult = {
  ok: boolean
  message: string
  sample?: string
}

export type IndexStatus = {
  repoRoot: string
  indexDir: string
  fileCount: number
  symbolCount: number
  embeddedChunkCount: number
  ready: boolean
}

export type IndexRebuildResult = {
  status: IndexStatus
  stats: {
    added: number
    changed: number
    removed: number
    unchanged: number
  }
}

export type SecretStorageMode = "file" | "keychain"

export type ProviderConfigView = {
  preferredProvider?: string
  baseUrl?: string
  region?: string
  defaultModel?: string
  cwd?: string
  configuredProviders: string[]
  hasAnyKey: boolean
  plugins: PluginPrefs
  fallbackModels: string[]
  defaultIsolation?: IsolationPolicy | string
  secretStorage: SecretStorageMode
}

export type SaveProviderConfigInput = {
  preferredProvider: string
  apiKey?: string
  baseUrl?: string
  region?: string
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

export type CopilotAuthStatus = {
  signedIn: boolean
}

export type CopilotAuthStart = {
  sessionId: string
  userCode: string
  verificationUri: string
  expiresIn: number
}

export type ChatgptAuthStatus = {
  signedIn: boolean
}

export type ChatgptAuthStart = {
  sessionId: string
  userCode: string
  verificationUri: string
}

export type ProviderProfileView = {
  id: string
  label: string
  provider: string
  baseUrl?: string
  region?: string
  defaultModel?: string
  fallbackModels?: string
  defaultIsolation?: IsolationPolicy | string
  hasKey: boolean
  isActive: boolean
}

export type ProviderProfileInput = {
  id?: string
  label: string
  provider: string
  apiKey?: string
  baseUrl?: string
  region?: string
  defaultModel?: string
  fallbackModels?: string
  defaultIsolation?: IsolationPolicy | string
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
  reuseWorkspaceId?: string | null
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

export type GitFileStatus = {
  path: string
  status: string
  added?: number
  removed?: number
}

export type GitStatusSummary = {
  files: GitFileStatus[]
  totalCount: number
  totalAdded: number
  totalRemoved: number
  truncated: boolean
}

export type FileHit = {
  path: string
  name: string
  isDir?: boolean
}

export type PromptAttachment = {
  path: string
  kind: "image" | "file" | "directory"
  name?: string
  mediaType?: string
}

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
  effort?: string
  composerMode?: string
}

export const EFFORT_LEVELS = ["low", "medium", "high", "xhigh", "max"] as const
export type EffortLevel = (typeof EFFORT_LEVELS)[number]

export const effortLabel = (effort: string): string => {
  if (effort === "xhigh") return "X-High"
  return effort.charAt(0).toUpperCase() + effort.slice(1)
}

export type RespondPermissionInput = {
  sessionId: string
  requestId: string
  decision: string
  reason?: string
}

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

export type McpServerDto = {
  id: string
  command: string
  args: string[]
  env: Record<string, string>
  secretEnv?: Record<string, string>
  secretArgs?: string[]
  configuredSecretEnv?: string[]
  hasSecretArgs?: boolean
  enabled: boolean
}

export type MemoryEntryDto = {
  id: string
  title: string
  content?: string
  updatedAtMs?: number
  expiresAtMs?: number
}

export type RespondQuestionInput = {
  sessionId: string
  requestId: string
  answers: Answer[]
}

export type TerminalInfo = {
  id: string
  cwd: string
  createdAtMs: number
  cwdFallbackFrom?: string
}

export type TerminalOutputEvent = { id: string; data: string }

export type TerminalExitEvent = { id: string; exitCode?: number }

export type BrowserStateEvent = {
  url: string
  title: string | null
  loading: boolean
  canGoBack: boolean
  canGoForward: boolean
  error?: { host: string; message: string } | null
}

export type {
  BrowserDesignEvent,
  BrowserDesignSelectEvent,
  BrowserDesignExitEvent,
  BrowserDomElement,
  BrowserDomRect,
} from "../browserDesign"

export type ReviewPatchTarget = "worktree" | "base"

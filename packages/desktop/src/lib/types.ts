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

/** `commands::BackgroundProcessDto` — one background process started via
 * `Bash`'s `run_in_background`, tracked by the engine's
 * `BackgroundProcessRegistry` and reachable via `background_list`/
 * `background_kill` (`src-tauri/src/commands.rs`). */
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

/** `agentloop_contracts::VerdictOutcome` — the `Verify` tool's graded result. */
export type VerdictOutcome = "pass" | "fail" | "inconclusive"

/** `agentloop_contracts::VerificationVerdict`, read out of a completed `Verify`
 * call's `ToolOutput.structured` (see `agentloop_core::tool::VERIFIER_TOOL_NAME`
 * and `EngineService::verify_goal_progress` in `packages/engine/crates/engine/src/lib.rs`). */
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
  | { kind: "compaction_boundary"; summary: unknown }
  | { kind: "model_fallback"; from: string; to?: string; reason: EngineError }
  /** Ephemeral, live-broadcast only — never persisted to replay/JSONL. Fires
   * once per retry attempt right before the engine sleeps that attempt's
   * backoff. Success = normal stream events resume; exhaustion is signaled
   * separately via `model_fallback` or a terminal turn error. */
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

/** Secret storage backend: `"file"` stores the encryption key in a local
 * file (no OS prompts, ever); `"keychain"` stores it in the OS Keychain
 * (protected by the OS, but may prompt). See
 * `src-tauri/src/secrets.rs::SecretStorageMode`. */
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

/** A named provider connection ("profile") — e.g. "AWS work" (Bedrock, key A,
 * us-east-1) vs. "AWS personal" (Bedrock, key B, eu-west-1). The API key
 * itself is never returned; `hasKey` reports whether one is stored. See
 * `src-tauri/src/config.rs::ProviderProfile`. */
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

/** Create/update input for one profile (`profile_upsert`). `id` empty/omitted
 * creates a new profile; `apiKey` empty/omitted keeps the existing stored key
 * on update. Also the shape `validate_profile` takes, so Validate always
 * checks the exact values currently in the form. */
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

export type ComposerMode = "agent" | "plan" | "ask" | "flex"

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
  /** Wire values of contracts::request::Effort (`#[serde(rename_all =
   * "lowercase")]`): "low" | "medium" | "high" | "xhigh" | "max". Omitted =
   * engine default. */
  effort?: string
  /** The composer mode the user picked ("agent" | "plan" | "ask" | "flex"),
   * separate from `permissionMode` (its derived wire value). Backend uses it
   * only to decide whether to append the Flex orchestrator system prompt
   * (see `commands.rs::prompt`); it does not affect permission handling on
   * its own. */
  composerMode?: string
}

/** Mirrors contracts::request::Effort's serde wire values (lowercase),
 * ordered low → high. "Default" (unset) is represented as `null` in state,
 * not a member of this list. */
export const EFFORT_LEVELS = ["low", "medium", "high", "xhigh", "max"] as const
export type EffortLevel = (typeof EFFORT_LEVELS)[number]

/** Display label for an effort wire value ("xhigh" -> "X-High"). */
export const effortLabel = (effort: string): string => {
  if (effort === "xhigh") return "X-High"
  return effort.charAt(0).toUpperCase() + effort.slice(1)
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

export type AppRoute =
  | "chat"
  | "settings"
  | "customize"
  | "automations"
  | "memory"
  | "welcome"

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

/** A user-configured MCP (Model Context Protocol) server (stdio transport
 * only — see `agentloop_mcp::McpServerConfig` / `commands::McpServerDto`).
 * Its tools are bridged into the native tool registry as `<id>__<tool>` at
 * the next engine service rebuild (saving/removing rebuilds it; there is no
 * hot-reload of already-open sessions). */
export type McpServerDto = {
  id: string
  command: string
  args: string[]
  env: Record<string, string>
  enabled: boolean
}

/** A durable note the `learning` plugin's `MemoryWrite` tool persisted —
 *  loads into every future session's system prompt. */
export type MemoryEntryDto = {
  /** The note's name (file stem), e.g. `user-preferences`. */
  id: string
  /** First non-empty line of the note, used as a title in the list view. */
  title: string
  /** Full markdown body. `undefined` in the list view — call `memoryGet`. */
  content?: string
  /** Milliseconds since epoch, from the file's last-modified time. */
  updatedAtMs?: number
  /** Milliseconds since epoch when this entry expires and is purged.
   * `undefined` = long-term (never expires). Sourced from a sidecar
   * `expiry.json` file next to the `.md` notes — never from the note body
   * itself, since the engine's prompt loader reads `.md` files raw. */
  expiresAtMs?: number
}

/** Preset TTLs offered by the memory expiry menu, mapped to absolute
 * `expiresAtMs` at selection time. `"forever"` clears any expiry. */
export type MemoryTtlPreset = "forever" | "1d" | "1w" | "30d"

const MEMORY_TTL_MS: Record<Exclude<MemoryTtlPreset, "forever">, number> = {
  "1d": 24 * 60 * 60 * 1000,
  "1w": 7 * 24 * 60 * 60 * 1000,
  "30d": 30 * 24 * 60 * 60 * 1000,
}

/** Absolute expiry timestamp for a TTL preset selected "now", or `undefined`
 * for `"forever"` (never expires). */
export const memoryExpiryFromPreset = (
  preset: MemoryTtlPreset,
  now: number = Date.now(),
): number | undefined => {
  if (preset === "forever") return undefined
  return now + MEMORY_TTL_MS[preset]
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

/** One subagent task parsed from a `RunWorkflow` call's raw input JSON. */
export type WorkflowStepTaskInput = {
  role: string
  prompt: string
  label?: string
}

/** One step of a `RunWorkflow` plan, parsed from `ToolCall.input.steps`. */
export type WorkflowStepInput =
  | { kind: "task"; task: WorkflowStepTaskInput }
  | { kind: "parallel"; tasks: WorkflowStepTaskInput[] }

/** Lifecycle of one subagent slot consumed by a workflow step, tracked in
 * arrival order (engine emits no step index — see WorkflowGroup.tsx). */
export type WorkflowSubagentSlot = {
  childSession: SessionId
  task: string
  role?: string
  phase: "started" | "completed"
  summary?: TurnSummary
  children: TimelineRow[]
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
      /** The parent tool call that spawned this subagent, when tool-driven
       * (e.g. `Task`). Used to route it into a `workflow` row instead of
       * rendering it as a top-level subagent block. */
      callId?: ToolCallId
      phase: "started" | "completed"
      summary?: TurnSummary
      children: TimelineRow[]
      tsMs: number
    }
  | {
      type: "workflow"
      id: string
      callId: ToolCallId
      toolName: string
      steps: WorkflowStepInput[]
      status: ToolCallStatus
      /** Subagent slots observed so far, in arrival order — consumed
       * front-to-back to infer each step's progress (no step index/total
       * exists on the wire; see WorkflowGroup.tsx). */
      subagents: WorkflowSubagentSlot[]
      tsMs: number
    }
  | {
      type: "verdict"
      id: string
      callId: ToolCallId
      /** Pending/running while the `Verify` call is in flight; a settled
       * verdict only exists once `status.state === "completed"` and the
       * call's `result.structured` parsed as a `VerificationVerdict`. */
      status: ToolCallStatus
      verdict?: VerificationVerdict
      tsMs: number
    }
  | {
      type: "checkpoint"
      id: string
      snapshotId: string
      turnId?: TurnId
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

/** Title derived from the first user prompt . */
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
  /** Navigation/load failure, when detected. Native emits this after
   * `PageLoadEvent::Finished` when the document looks like a chrome-error /
   * about:neterror / connection-refused page (eval probe). Preview mock
   * also sets it for `FAILING_MOCK_HOST`. */
  error?: { host: string; message: string } | null
}

// Per-file / per-hunk review actions (Changes tab Keep/Undo — the reference design pattern).

/** Where a `review_apply_patch` call applies its patch: the session's
 * working dir (worktree root if isolated, else the repo itself), or the
 * isolated session's base repo (errors if the session isn't isolated). */
export type ReviewPatchTarget = "worktree" | "base"

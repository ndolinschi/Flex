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

/** Record of a context compaction (`compaction_boundary` payload). */
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
  | { kind: "compaction_boundary"; summary: CompactionSummary }
  /** Ephemeral, live-broadcast only — never persisted. Fires right before
   * the summarizer runs so the UI can show "Compacting context…" until the
   * following `compaction_boundary` (or turn end / error) lands. */
  | { kind: "compaction_started"; strategy: string }
  /** Ephemeral — UI shows "Indexing repository…" until `indexing_completed`. */
  | { kind: "indexing_started"; reason: string }
  /** Persisted — settled "Indexed N files" card in the chat timeline. */
  | { kind: "indexing_completed"; added: number; changed: number; removed: number; unchanged: number }
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
  /** Peer-to-peer agent message — persisted on the recipient session log. */
  | { kind: "peer_message"; id: string; from: SessionId; to?: SessionId; thread_id?: string; content: string; about_path?: string }
  /** Auto/router proposed a composer-mode switch; UI shows a veto countdown. */
  | { kind: "mode_switch_proposed"; id: string; mode: string; reason: string; timeout_ms: number }
  /** The proposed mode switch was accepted and applied. */
  | { kind: "mode_switch_applied"; id: string; mode: string }
  /** The proposed mode switch was vetoed by the user or timed out. */
  | { kind: "mode_switch_rejected"; id: string; mode: string; reason?: string }
  /** Auto/router changed the session's model and/or effort mid-turn. */
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

// Desktop command DTOs (camelCase serde)

export type PluginPrefs = {
  search: boolean
  index: boolean
  /** Inject top-k indexed snippets into each turn's first user message. Default off. */
  autoContext: boolean
  /**
   * Rescan/update the on-disk index on every SearchCode / FindSymbol / RepoMap.
   * Default off — reuse a warm index across chats; Rebuild from Settings to refresh.
   */
  autoUpdateIndex: boolean
  learning: boolean
  /** When Learning is on: force-ask on SkillSave / MemoryWrite. Default off. */
  learningRequireHumanApproval: boolean
  /**
   * When Learning is on: require a passing Verify before memory writes.
   * Default off; pair with Verifier enabled.
   */
  learningRequireVerifiedMemory: boolean
  verifier: boolean
  /** Embedded Browser panel tools (navigate / screenshot / eval / console). Default off. */
  browser: boolean
  /** OS computer-use tools with animated agent cursor. Default off. */
  computer: boolean

  // --- Agent coordination / auto mode ---

  /** Peer agent messaging + SwitchMode tools. Default off. */
  messaging: boolean
  /** Council mode — enables Verifier for second-opinion grading. Default off. */
  council: boolean
  /** Composer Auto routing — shows "Auto" in the model picker. Default off. */
  autoMode: boolean
  /** Model used in Auto mode (e.g. "anthropic/claude-sonnet-4-5"). */
  autoModeRouterModel?: string
  /** Proactive auto-compaction when context nears threshold. Default true. */
  autoCompact: boolean
  /** Context % threshold for proactive compaction (1–100). Default 85. */
  autoCompactThresholdPercent: number
  /** Compaction strategy: "standard" | "turn_pair". Default "standard". */
  compactionMode: string
  /** Ms the UI waits before auto-accepting a ModeSwitchProposed. Default 2000. */
  modeSwitchVetoMs: number
  /** System delegation rules for Auto mode (empty = use built-in defaults). */
  delegationRules: string

  // --- Cost-tier routing ---

  /** Which cost tier SetRouting may escalate to: "low" | "medium" | "high" | "auto". Default "auto". */
  costMode: string
  /** Models at the low cost tier (fast, cheap). Auto starts here. */
  costModelsLow: string[]
  /** Models at the medium cost tier (balanced). */
  costModelsMedium: string[]
  /** Models at the high cost tier (powerful, expensive). */
  costModelsHigh: string[]
}

/** Desktop UI prefs for ghost-text prompt completion (not an engine plugin). */
export type InlineCompletionPrefs = {
  enabled: boolean
  providerId?: string
  modelId?: string
  /** User dismissed setup without connecting — stop auto-prompting. */
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

/** Result of `copilot_auth_status` / `copilot_auth_wait`. */
export type CopilotAuthStatus = {
  signedIn: boolean
}

/** Public half of a pending device-flow session (`copilot_auth_start`). The
 * private device code stays in the Tauri backend. */
export type CopilotAuthStart = {
  sessionId: string
  userCode: string
  verificationUri: string
  expiresIn: number
}

/** Result of `chatgpt_auth_status` / `chatgpt_auth_wait`. */
export type ChatgptAuthStatus = {
  signedIn: boolean
}

/** Public half of a pending ChatGPT headless OAuth session. */
export type ChatgptAuthStart = {
  sessionId: string
  userCode: string
  verificationUri: string
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
  /** Attach an existing worktree (from `listWorkspaces`) on first prompt
   * instead of provisioning a new one. Ignored when `isolation` doesn't
   * resolve to a policy that wants isolation. */
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

/** One changed file from `git_status` (Changes panel). */
export type GitFileStatus = {
  /** Path relative to the session cwd. */
  path: string
  /** Porcelain letter: "M" | "A" | "D" | "R" | "?" (untracked). */
  status: string
  added?: number
  removed?: number
}

/** `git_status`/`git_status_since_baseline` response. `files` is capped at a
 * server-side row limit (see `MAX_STATUS_FILES` in commands.rs) so a session
 * with hundreds of changed files (e.g. after scaffolding a project) never
 * asks the UI to render every row — `totalCount`/`totalAdded`/`totalRemoved`
 * are computed over the *full*, untruncated set so the aggregate file-count
 * and +/- badges stay correct regardless of the cap, and `truncated` tells
 * the UI whether to show a "+N more" indicator. */
export type GitStatusSummary = {
  files: GitFileStatus[]
  totalCount: number
  totalAdded: number
  totalRemoved: number
  truncated: boolean
}

/** A file or folder match from `list_files`, used by composer @-mentions and Files browse. */
export type FileHit = {
  /** Path relative to the session cwd, forward-slashed. */
  path: string
  /** Basename, shown as the primary label. */
  name: string
  /** True for directories (folder icon in the @ tray / Files tree). */
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
  /** Wire values of contracts::request::Effort (`#[serde(rename_all =
   * "lowercase")]`): "low" | "medium" | "high" | "xhigh" | "max". Omitted =
   * engine default. */
  effort?: string
  /** The composer mode the user picked ("agent" | "plan" | "ask" | "flex" | "debug"),
   * separate from `permissionMode` (its derived wire value). Backend appends
   * mode-specific system prompts for flex / plan / debug (see
   * `commands.rs::prompt`); it does not affect permission handling on its own. */
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

/** A user-configured MCP (Model Context Protocol) server (stdio transport
 * only — see `agentloop_mcp::McpServerConfig` / `commands::McpServerDto`).
 * Its tools are bridged into the native tool registry as `<id>__<tool>` at
 * the next engine service rebuild (saving/removing rebuilds it; there is no
 * hot-reload of already-open sessions). */
export type McpServerDto = {
  id: string
  command: string
  args: string[]
  /** Non-secret env vars (persisted in the MCP TOML file). */
  env: Record<string, string>
  /**
   * Secret env values to write into the encrypted secrets store.
   * On upsert: non-empty overwrites; empty string keeps the existing secret.
   * List responses leave this empty — see `configuredSecretEnv`.
   */
  secretEnv?: Record<string, string>
  /**
   * Secret positional-arg values appended after `args` at resolve time
   * (e.g. Postgres connection string). Omitted on list.
   */
  secretArgs?: string[]
  /** Env key names that have a stored secret (values never returned). */
  configuredSecretEnv?: string[]
  /** Whether a secret positional-args suffix is stored for this server. */
  hasSecretArgs?: boolean
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

export type RespondQuestionInput = {
  sessionId: string
  requestId: string
  answers: Answer[]
}

// Right-panel Terminal + Browser features (desktop-only, camelCase serde)

export type TerminalInfo = {
  id: string
  cwd: string
  createdAtMs: number
  /** Set when the session cwd was missing and the PTY opened in home instead. */
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
  /** Navigation/load failure, when detected. Native emits this after
   * `PageLoadEvent::Finished` when the document looks like a chrome-error /
   * about:neterror / connection-refused page (eval probe). Preview mock
   * also sets it for `FAILING_MOCK_HOST`. */
  error?: { host: string; message: string } | null
}

export type {
  BrowserDesignEvent,
  BrowserDesignSelectEvent,
  BrowserDesignExitEvent,
  BrowserDomElement,
  BrowserDomRect,
} from "../browserDesign"

// Per-file / per-hunk review actions (Changes tab Keep/Undo — the reference design pattern).

/** Where a `review_apply_patch` call applies its patch: the session's
 * working dir (worktree root if isolated, else the repo itself), or the
 * isolated session's base repo (errors if the session isn't isolated). */
export type ReviewPatchTarget = "worktree" | "base"

import type {
  MessageId,
  ToolCallId,
  ToolCall,
  PlanEntry,
  TurnId,
  TurnSummary,
  VerificationVerdict,
  ToolCallStatus,
  SessionId,
  PermissionRequestId,
  PermissionDecisionKind,
  Question,
  EngineError,
} from "./wire"

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

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
  toolProgress: Record<ToolCallId, string>
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

export type WorkflowStepTaskInput = {
  role: string
  prompt: string
  label?: string
}

export type WorkflowStepInput =
  | { kind: "task"; task: WorkflowStepTaskInput }
  | { kind: "parallel"; tasks: WorkflowStepTaskInput[] }

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
  | {
      type: "thinking"
      id: string
      messageId: MessageId
      text: string
      tsMs: number
      durationMs?: number
    }
  | { type: "tool"; id: string; call: ToolCall; tsMs: number }
  | { type: "plan"; id: string; entries: PlanEntry[]; tsMs: number }
  | { type: "turn"; id: string; turnId: TurnId; phase: "started" | "completed"; summary?: TurnSummary; tsMs: number }
  | { type: "error"; id: string; error: EngineError; tsMs: number }
  | { type: "fallback"; id: string; from: string; to?: string; reason: string; tsMs: number }
  | { type: "command"; id: string; name: string; args: string; tsMs: number }
  | { type: "meta"; id: string; text: string; tsMs: number }
  | {
      type: "compaction"
      id: string
      summaryMarkdown: string
      strategy: string
      tokensBefore?: number
      tokensAfter?: number
      tsMs: number
    }
  | {
      type: "indexing"
      id: string
      added: number
      changed: number
      removed: number
      unchanged: number
      tsMs: number
    }
  | {
      type: "subagent"
      id: string
      childSession: SessionId
      task: string
      role?: string
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
      subagents: WorkflowSubagentSlot[]
      tsMs: number
    }
  | {
      type: "verdict"
      id: string
      callId: ToolCallId
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
  | {
      type: "peer_message"
      id: string
      from: SessionId
      to?: SessionId
      threadId?: string
      content: string
      aboutPath?: string
      tsMs: number
    }
  | {
      type: "mode_switch"
      id: string
      state: string
      mode: string
      reason?: string
      tsMs: number
    }
  | {
      type: "routing_changed"
      id: string
      model?: string
      effort?: string
      reason: string
      tsMs: number
    }

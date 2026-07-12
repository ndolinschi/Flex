import type { AgentEvent, StreamingBuffers } from "../types"

export const applyEventToStreaming = (
  buffers: StreamingBuffers,
  payload: AgentEvent,
  materializedMessageIds: Set<string>,
): StreamingBuffers => {
  const next: StreamingBuffers = {
    markdown: { ...buffers.markdown },
    thinking: { ...buffers.thinking },
    toolCalls: { ...buffers.toolCalls },
    toolProgress: { ...buffers.toolProgress },
    toolArgs: { ...buffers.toolArgs },
  }

  switch (payload.kind) {
    case "markdown_delta": {
      if (!materializedMessageIds.has(payload.message_id)) {
        const prev = next.markdown[payload.message_id] ?? ""
        next.markdown[payload.message_id] = prev + payload.text
      }
      break
    }
    case "thinking_delta": {
      const prev = next.thinking[payload.message_id] ?? ""
      next.thinking[payload.message_id] = prev + payload.text
      break
    }
    case "text_snapshot": {
      next.markdown[payload.message_id] = payload.text
      break
    }
    case "assistant_message": {
      delete next.markdown[payload.message_id]
      delete next.thinking[payload.message_id]
      break
    }
    case "tool_progress": {
      next.toolProgress[payload.call_id] = payload.note
      break
    }
    case "tool_args_delta": {
      const prev = next.toolArgs[payload.call_id] ?? ""
      next.toolArgs[payload.call_id] = prev + payload.json_fragment
      break
    }
    case "tool_call_updated": {
      next.toolCalls[payload.call.id] = payload.call
      // Once a call settles, drop its transient progress/args buffers.
      const state = payload.call.status.state
      if (
        state === "completed" ||
        state === "failed" ||
        state === "denied" ||
        state === "cancelled"
      ) {
        delete next.toolProgress[payload.call.id]
        delete next.toolArgs[payload.call.id]
      }
      break
    }
    case "turn_completed": {
      next.markdown = {}
      next.thinking = {}
      next.toolProgress = {}
      next.toolArgs = {}
      break
    }
    default:
      break
  }

  return next
}

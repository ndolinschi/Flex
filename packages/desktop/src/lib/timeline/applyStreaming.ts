import type { AgentEvent, StreamingBuffers } from "../types"

const isTerminalToolState = (state: string): boolean =>
  state === "completed" ||
  state === "failed" ||
  state === "denied" ||
  state === "cancelled"

/**
 * Apply a stream event into `StreamingBuffers` with structural sharing:
 * unchanged maps keep their previous reference; fully unchanged input returns
 * the original `buffers` object.
 */
export const applyEventToStreaming = (
  buffers: StreamingBuffers,
  payload: AgentEvent,
  materializedMessageIds: Set<string>,
): StreamingBuffers => {
  switch (payload.kind) {
    case "markdown_delta": {
      if (materializedMessageIds.has(payload.message_id)) return buffers
      const prev = buffers.markdown[payload.message_id] ?? ""
      return {
        ...buffers,
        markdown: {
          ...buffers.markdown,
          [payload.message_id]: prev + payload.text,
        },
      }
    }
    case "thinking_delta": {
      const prev = buffers.thinking[payload.message_id] ?? ""
      return {
        ...buffers,
        thinking: {
          ...buffers.thinking,
          [payload.message_id]: prev + payload.text,
        },
      }
    }
    case "text_snapshot": {
      if (buffers.markdown[payload.message_id] === payload.text) return buffers
      return {
        ...buffers,
        markdown: {
          ...buffers.markdown,
          [payload.message_id]: payload.text,
        },
      }
    }
    case "assistant_message": {
      const hasMarkdown = payload.message_id in buffers.markdown
      const hasThinking = payload.message_id in buffers.thinking
      if (!hasMarkdown && !hasThinking) return buffers
      const next: StreamingBuffers = { ...buffers }
      if (hasMarkdown) {
        const markdown = { ...buffers.markdown }
        delete markdown[payload.message_id]
        next.markdown = markdown
      }
      if (hasThinking) {
        const thinking = { ...buffers.thinking }
        delete thinking[payload.message_id]
        next.thinking = thinking
      }
      return next
    }
    case "tool_progress": {
      if (buffers.toolProgress[payload.call_id] === payload.note) return buffers
      return {
        ...buffers,
        toolProgress: {
          ...buffers.toolProgress,
          [payload.call_id]: payload.note,
        },
      }
    }
    case "tool_args_delta": {
      const prev = buffers.toolArgs[payload.call_id] ?? ""
      return {
        ...buffers,
        toolArgs: {
          ...buffers.toolArgs,
          [payload.call_id]: prev + payload.json_fragment,
        },
      }
    }
    case "tool_call_updated": {
      const callId = payload.call.id
      const state = payload.call.status.state
      const terminal = isTerminalToolState(state)
      const sameCall = buffers.toolCalls[callId] === payload.call
      const hadProgress = callId in buffers.toolProgress
      const hadArgs = callId in buffers.toolArgs
      if (sameCall && (!terminal || (!hadProgress && !hadArgs))) {
        return buffers
      }

      const next: StreamingBuffers = {
        ...buffers,
        toolCalls: {
          ...buffers.toolCalls,
          [callId]: payload.call,
        },
      }
      if (terminal) {
        if (hadProgress) {
          const toolProgress = { ...buffers.toolProgress }
          delete toolProgress[callId]
          next.toolProgress = toolProgress
        }
        if (hadArgs) {
          const toolArgs = { ...buffers.toolArgs }
          delete toolArgs[callId]
          next.toolArgs = toolArgs
        }
      }
      return next
    }
    case "turn_completed": {
      if (
        Object.keys(buffers.markdown).length === 0 &&
        Object.keys(buffers.thinking).length === 0 &&
        Object.keys(buffers.toolProgress).length === 0 &&
        Object.keys(buffers.toolArgs).length === 0
      ) {
        return buffers
      }
      return {
        ...buffers,
        markdown: {},
        thinking: {},
        toolProgress: {},
        toolArgs: {},
      }
    }
    default:
      return buffers
  }
}

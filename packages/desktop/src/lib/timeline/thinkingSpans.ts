import type { SessionEvent } from "../types"

/** First/last `thinking_delta` timestamp per message, used to derive "Thought for Xs". */
export type ThinkingSpan = { startMs: number; endMs: number }

/**
 * Track thinking-delta timestamps so the timeline can show "Thought for Xs".
 * Only live streams carry per-delta timestamps — replayed history collapses
 * thinking into a single materialized row with no span, so those messages
 * are simply absent from the map (ThinkingBlock falls back to plain "Thought").
 */
export const trackThinkingSpan = (
  spans: Record<string, ThinkingSpan>,
  event: SessionEvent,
): Record<string, ThinkingSpan> => {
  if (event.payload.kind !== "thinking_delta") return spans
  const { message_id } = event.payload
  const existing = spans[message_id]
  if (!existing) {
    return {
      ...spans,
      [message_id]: { startMs: event.ts_ms, endMs: event.ts_ms },
    }
  }
  if (event.ts_ms === existing.endMs) return spans
  return {
    ...spans,
    [message_id]: { ...existing, endMs: event.ts_ms },
  }
}

export const durationsFromSpans = (
  spans: Record<string, ThinkingSpan>,
): Record<string, number> => {
  const out: Record<string, number> = {}
  for (const [messageId, span] of Object.entries(spans)) {
    out[messageId] = Math.max(0, span.endMs - span.startMs)
  }
  return out
}

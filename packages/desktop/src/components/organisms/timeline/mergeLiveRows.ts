import type { StreamingBuffers, TimelineRow } from "../../../lib/types"

export type SessionLogRow = { id: string; text: string; tsMs: number }

/**
 * Merge materialized timeline rows with in-flight streaming buffers and
 * client-side session log rows. Builds id Sets once so each streaming key is
 * O(1) against materialization instead of O(rows) `.some` scans.
 */
export const mergeLiveRows = (
  rows: TimelineRow[],
  streaming: StreamingBuffers,
  sessionLogRows?: SessionLogRow[],
  nowMs: number = Date.now(),
): TimelineRow[] => {
  const thinkingOrAssistantIds = new Set<string>()
  const assistantIds = new Set<string>()
  const toolIds = new Set<string>()

  for (const r of rows) {
    if (r.type === "thinking" || r.type === "assistant") {
      thinkingOrAssistantIds.add(r.messageId)
      if (r.type === "assistant") assistantIds.add(r.messageId)
    } else if (r.type === "tool") {
      toolIds.add(r.call.id)
    }
  }

  const extra: TimelineRow[] = []

  for (const call of Object.values(streaming.toolCalls)) {
    // RunWorkflow calls materialize as a `workflow` row and Verify calls as
    // a `verdict` row (both in useSessionEvents) — never a plain `tool`
    // row — skip the generic live-tool fallback here for both.
    if (call.tool_name === "RunWorkflow" || call.tool_name === "Verify") continue
    if (toolIds.has(call.id)) continue
    extra.push({
      type: "tool",
      id: `live-tool:${call.id}`,
      call,
      tsMs: nowMs,
    })
  }

  for (const [messageId, text] of Object.entries(streaming.markdown)) {
    if (!text) continue
    if (assistantIds.has(messageId)) continue
    extra.push({
      type: "assistant",
      id: `live-assistant:${messageId}`,
      messageId,
      text,
      tsMs: nowMs,
    })
  }

  // Thinking last among live-only extras so in-flight reasoning sits under
  // tools / provisional narration (buildDisplayItems also reorders
  // materialized thinking to the end of each work group).
  for (const [messageId, text] of Object.entries(streaming.thinking)) {
    if (!text) continue
    // Skip once either a materialized thinking row OR the assistant
    // message for this id exists — otherwise a thinking-only
    // assistant_message (no markdown) would duplicate the live row.
    if (thinkingOrAssistantIds.has(messageId)) continue
    extra.push({
      type: "thinking",
      id: `live-thinking:${messageId}`,
      messageId,
      text,
      tsMs: nowMs,
    })
  }

  const logRows: TimelineRow[] = (sessionLogRows ?? []).map((log) => ({
    type: "meta",
    id: log.id,
    text: log.text,
    tsMs: log.tsMs,
  }))

  // `rows` is already in authoritative event order — never re-sort. Only
  // `logRows` need slotting in by timestamp; live-only `extra` rows always
  // represent the newest in-flight content and belong after materialized rows.
  if (logRows.length === 0) return [...rows, ...extra]

  const withLogs = [...rows]
  for (const log of logRows) {
    let insertAt = withLogs.length
    for (let i = withLogs.length - 1; i >= 0; i--) {
      if (withLogs[i].tsMs <= log.tsMs) {
        insertAt = i + 1
        break
      }
      insertAt = i
    }
    withLogs.splice(insertAt, 0, log)
  }
  return [...withLogs, ...extra]
}

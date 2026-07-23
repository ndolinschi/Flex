import type { StreamingBuffers, TimelineRow } from "../../../lib/types"

export type SessionLogRow = { id: string; text: string; tsMs: number }

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

  for (const [messageId, text] of Object.entries(streaming.thinking)) {
    if (!text) continue
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

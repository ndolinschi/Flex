/** Terminal id for a session's read-only agent terminal (mirrors `exec_chunk`). */
export const agentTerminalId = (sessionId: string): string => `agent:${sessionId}`

/** Last `call_id` seen per agent-terminal key — used to detect a new command
 * boundary (insert a separator) and to gate the one-shot auto-activate. */
export const lastCallIdByAgentKey = new Map<string, string>()

/** Call ids that have already triggered the one-shot right-panel auto-activate. */
export const autoActivatedCallIds = new Set<string>()

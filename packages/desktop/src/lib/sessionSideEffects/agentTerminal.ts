export const agentTerminalId = (sessionId: string): string => `agent:${sessionId}`

export const lastCallIdByAgentKey = new Map<string, string>()

export const autoActivatedCallIds = new Set<string>()

import type { CreateSessionInput, IsolationPolicy, SessionMeta } from "./types"
import { DEFAULT_SESSION_TITLE, isPristineSession } from "./types"

export const isSessionNotFoundError = (message: string): boolean =>
  /session\s+\S+\s+not found/i.test(message) || /session not found/i.test(message)

export const findDraftSession = (
  sessions: SessionMeta[],
  cwd?: string | null,
): SessionMeta | undefined => {
  const key = cwd?.trim() || ""
  return sessions.find((s) => {
    if (!isPristineSession(s)) return false
    const sessionCwd = s.cwd?.trim() || ""
    return sessionCwd === key
  })
}

export const resolveCreateCwd = (
  sessions: SessionMeta[],
  activeSessionId: string | null,
  recentCwds: string[],
  explicitCwd?: string,
): string | undefined => {
  if (explicitCwd?.trim()) return explicitCwd.trim()
  if (recentCwds[0]?.trim()) return recentCwds[0].trim()
  const active = sessions.find((s) => s.id === activeSessionId)
  return active?.cwd?.trim() || undefined
}

export const newAgentCreateInput = (
  cwd?: string,
  model?: string | null,
  isolation?: IsolationPolicy | null,
  reuseWorkspaceId?: string | null,
): CreateSessionInput => ({
  title: DEFAULT_SESSION_TITLE,
  ...(cwd ? { cwd } : {}),
  ...(model ? { model } : {}),
  ...(isolation ? { isolation } : {}),
  ...(reuseWorkspaceId ? { reuseWorkspaceId } : {}),
})

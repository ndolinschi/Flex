import type { CreateSessionInput, SessionMeta } from "./types"
import { DEFAULT_SESSION_TITLE, isDefaultSessionTitle } from "./types"

/** Find an unused "New Agent" session for this project (Cursor: one draft per cwd). */
export const findDraftSession = (
  sessions: SessionMeta[],
  cwd?: string | null,
): SessionMeta | undefined => {
  const key = cwd?.trim() || ""
  return sessions.find((s) => {
    if (!isDefaultSessionTitle(s.title)) return false
    const sessionCwd = s.cwd?.trim() || ""
    return sessionCwd === key
  })
}

/** Resolve cwd preference: explicit → recent → active session. */
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
): CreateSessionInput => ({
  title: DEFAULT_SESSION_TITLE,
  ...(cwd ? { cwd } : {}),
  ...(model ? { model } : {}),
})

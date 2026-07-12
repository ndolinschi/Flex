import { useMemo } from "react"
import { useSessions } from "../../../hooks/useSessions"

/** Discover distinct project cwds from live sessions — top-level only
    (subagent children carry a `parent_id` and their worktree paths aren't
    user projects), deduped in first-seen order. */
export const useProjectCwds = (): string[] => {
  const { sessions } = useSessions()
  return useMemo(() => {
    const seen = new Set<string>()
    const cwds: string[] = []
    for (const session of sessions) {
      if (session.parent_id) continue
      if (!session.cwd || seen.has(session.cwd)) continue
      seen.add(session.cwd)
      cwds.push(session.cwd)
    }
    return cwds
  }, [sessions])
}

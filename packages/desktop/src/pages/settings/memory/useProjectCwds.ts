import { useMemo } from "react"
import { useSessions } from "../../../hooks/useSessions"
import { projectCwd } from "../../../lib/sessionGrouping"

/** Discover distinct project cwds from live sessions — top-level only
    (subagent children carry a `parent_id` and their worktree paths aren't
    user projects), keyed by `base_cwd ?? cwd` so isolated worktrees collapse
    into the real repo, deduped in first-seen order. */
export const useProjectCwds = (): string[] => {
  const { sessions } = useSessions()
  return useMemo(() => {
    const seen = new Set<string>()
    const cwds: string[] = []
    for (const session of sessions) {
      if (session.parent_id) continue
      const key = projectCwd(session)
      if (key === "~" || seen.has(key)) continue
      seen.add(key)
      cwds.push(key)
    }
    return cwds
  }, [sessions])
}

import { useMemo } from "react"
import { useSessions } from "../../../hooks/useSessions"
import { projectCwd } from "../../../lib/sessionGrouping"

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

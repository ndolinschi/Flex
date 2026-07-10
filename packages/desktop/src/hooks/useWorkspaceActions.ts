import { useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import {
  discardIsolatedSession,
  integrateSession,
  toInvokeError,
} from "../lib/tauri"

/**
 * Shared integrate/discard actions for isolated-workspace sessions
 * (ContextBar isolation popover + right-panel Changes tab).
 */
export const useWorkspaceActions = (
  sessionId: string | null | undefined,
  onError?: (message: string) => void,
  onDone?: () => void,
) => {
  const [busy, setBusy] = useState(false)
  const queryClient = useQueryClient()

  const run = async (fn: (id: string) => Promise<unknown>) => {
    if (!sessionId || busy) return
    setBusy(true)
    try {
      await fn(sessionId)
      void queryClient.invalidateQueries({ queryKey: ["sessions"] })
      void queryClient.invalidateQueries({ queryKey: ["is-isolated"] })
      void queryClient.invalidateQueries({ queryKey: ["workspace-status"] })
      void queryClient.invalidateQueries({ queryKey: ["git-status"] })
      onDone?.()
    } catch (err) {
      onError?.(toInvokeError(err))
    } finally {
      setBusy(false)
    }
  }

  return {
    busy,
    integrate: () => run(integrateSession),
    discard: () => run(discardIsolatedSession),
  }
}

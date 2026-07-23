import { useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import {
  discardIsolatedSession,
  integrateSession,
  toInvokeError,
} from "../lib/tauri"
import { invalidateGitQueries } from "../lib/invalidateGitQueries"
import { useAppStore } from "../stores/appStore"
import { log } from "../lib/debug/log"

export const useWorkspaceActions = (
  sessionId: string | null | undefined,
  onError?: (message: string) => void,
  onDone?: () => void,
) => {
  const [busy, setBusy] = useState(false)
  const queryClient = useQueryClient()
  const pushToast = useAppStore((s) => s.pushToast)

  const run = async (
    fn: (id: string) => Promise<unknown>,
    successText: string,
  ) => {
    if (!sessionId || busy) return
    setBusy(true)
    try {
      await fn(sessionId)
      void queryClient.invalidateQueries({ queryKey: ["sessions"] })
      void queryClient.invalidateQueries({ queryKey: ["is-isolated"] })
      void queryClient.invalidateQueries({ queryKey: ["workspace-status"] })
      invalidateGitQueries(queryClient)
      pushToast(successText, "success")
      onDone?.()
    } catch (err) {
      const message = toInvokeError(err)
      log.error("git", "workspace action failed", {
        sessionId,
        error: message,
      })
      pushToast(message, "error")
      onError?.(message)
    } finally {
      setBusy(false)
    }
  }

  return {
    busy,
    integrate: () => run(integrateSession, "Changes integrated"),
    discard: () => run(discardIsolatedSession, "Workspace discarded"),
  }
}

export const invalidateReviewQueries = (
  queryClient: ReturnType<typeof useQueryClient>,
  path?: string,
) => {
  void queryClient.invalidateQueries({ queryKey: ["workspace-status"] })
  invalidateGitQueries(queryClient)
  if (path) {
    void queryClient.invalidateQueries({ queryKey: ["review-file-diff", path] })
  } else {
    void queryClient.invalidateQueries({ queryKey: ["review-file-diff"] })
  }
}

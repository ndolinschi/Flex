import { useState } from "react"
import {
  respondPermission,
  setTurnPermissionMode,
  toInvokeError,
} from "../lib/tauri"
import type { PendingPermission } from "../lib/types"
import { useAppStore } from "../stores/appStore"
import { log } from "../lib/debug/log"

/** Backend error when the engine no longer has this request in memory. */
const isStalePermissionError = (message: string): boolean =>
  message.toLowerCase().includes("no pending permission request")

/** Shared respond / allow-always bypass / stale-dismiss for permission HITL. */
export const usePermissionRespond = (permission: PendingPermission) => {
  const setPendingPermission = useAppStore((s) => s.setPendingPermission)
  const setSessionBypass = useAppStore((s) => s.setSessionBypass)
  const pushToast = useAppStore((s) => s.pushToast)
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)

  const respond = async (decision: string) => {
    setError(null)
    setIsSubmitting(true)
    try {
      // Always allow → also arm session + live-turn bypass so subsequent
      // tools in this run (and later turns) stop asking.
      if (decision === "allow_always") {
        setSessionBypass(permission.sessionId, true)
        try {
          await setTurnPermissionMode(
            permission.sessionId,
            "bypass_permissions",
          )
        } catch (err) {
          log.warn("session", "set_turn_permission_mode during allow_always", {
            sessionId: permission.sessionId,
            error: toInvokeError(err),
          })
        }
      }
      await respondPermission({
        sessionId: permission.sessionId,
        requestId: permission.requestId,
        decision,
      })
      setPendingPermission(null)
    } catch (err) {
      const message = toInvokeError(err)
      if (isStalePermissionError(message)) {
        log.warn("session", "permission respond stale — dismissing", {
          requestId: permission.requestId,
          sessionId: permission.sessionId,
          error: message,
        })
        setPendingPermission(null)
        pushToast("Permission request expired", "error")
      } else {
        log.error("session", "permission respond failed", {
          requestId: permission.requestId,
          sessionId: permission.sessionId,
          error: message,
        })
        setError(message)
      }
    } finally {
      setIsSubmitting(false)
    }
  }

  return {
    error,
    clearError: () => setError(null),
    isSubmitting,
    respond,
  }
}

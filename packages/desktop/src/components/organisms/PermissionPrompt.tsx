import { useState } from "react"
import { createPortal } from "react-dom"
import { X } from "lucide-react"
import { Button, IconButton } from "../atoms"
import { ErrorBanner } from "../molecules"
import {
  respondPermission,
  setTurnPermissionMode,
  toInvokeError,
} from "../../lib/tauri"
import type { PendingPermission } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { log } from "../../lib/debug/log"

/** Backend error when the engine no longer has this request in memory
 * (engine restarted / session gone) — the request itself, if it still
 * exists, will simply time out; there is nothing left to respond to. */
const isStalePermissionError = (message: string): boolean =>
  message.toLowerCase().includes("no pending permission request")

type PermissionPromptProps = {
  permission: PendingPermission
}

/** Engine sends `title` pre-formatted as "Allow `ToolName`?" (see
 * packages/engine/crates/loop/src/turn/tool_exec.rs). Split out the
 * backticked tool name so it renders in code style; anything that doesn't
 * match (e.g. the browser mock's plain "Allow Bash?") just renders as-is. */
const splitTitle = (
  title: string,
): { prefix: string; tool: string; suffix: string } | null => {
  const match = /^(.*?)`([^`]+)`(.*)$/.exec(title)
  if (!match) return null
  return { prefix: match[1], tool: match[2], suffix: match[3] }
}

/** Prefer a short human line over raw JSON blobs in the detail field. */
const formatDetail = (detail?: string): string | null => {
  if (!detail?.trim()) return null
  const trimmed = detail.trim()
  try {
    const parsed = JSON.parse(trimmed) as unknown
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      const obj = parsed as Record<string, unknown>
      for (const key of ["command", "cmd", "path", "file_path", "pattern"]) {
        const v = obj[key]
        if (typeof v === "string" && v.trim()) return v.trim()
      }
    }
  } catch {
    // plain text
  }
  // Collapse single-line JSON-ish noise.
  if (trimmed.startsWith("{") && trimmed.length > 120) {
    return `${trimmed.slice(0, 100)}…`
  }
  return trimmed
}

export const PermissionPrompt = ({ permission }: PermissionPromptProps) => {
  const setPendingPermission = useAppStore((s) => s.setPendingPermission)
  const setSessionBypass = useAppStore((s) => s.setSessionBypass)
  const pushToast = useAppStore((s) => s.pushToast)
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const detail = formatDetail(permission.detail)
  const titleParts = splitTitle(permission.title)

  const handleDecision = async (decision: string) => {
    setError(null)
    setIsSubmitting(true)
    try {
      // Always allow → also arm session + live-turn bypass so subsequent
      // tools in this run (and later turns) stop asking. The engine's
      // allow_always rule alone used to only match a first-word Bash
      // prefix (e.g. `cd *`), so `cd X && npm i` still re-prompted.
      if (decision === "allow_always") {
        setSessionBypass(permission.sessionId, true)
        try {
          await setTurnPermissionMode(
            permission.sessionId,
            "bypass_permissions",
          )
        } catch (err) {
          // No in-flight turn (already settled) — session flag still covers
          // the next prompt(); don't block resolving this request.
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
        // The engine-side request is gone (restart / session ended) — dismiss
        // instead of hard-blocking on an error that can never be resolved.
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

  const handleDismiss = () => {
    // Local dismiss only — does not respond. If the engine-side request still
    // exists it will simply time out / stay unanswered; this just guarantees
    // the user is never hard-blocked by a modal they can't get rid of.
    setPendingPermission(null)
  }

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="permission-title"
      className="pointer-events-none fixed inset-0 z-[90] flex items-end justify-center px-4 pb-28 sm:items-center sm:pb-0"
    >
      <div className="pointer-events-auto w-full max-w-[640px] animate-modal-in">
        <div className="relative rounded-xl border border-stroke-3 bg-panel p-3 shadow-lg">
        <IconButton
          label="Dismiss"
          onClick={handleDismiss}
          className="absolute right-2 top-2 z-10 h-6 w-6"
        >
          <X className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
        <h3 id="permission-title" className="pr-6 text-sm font-semibold text-ink">
          {titleParts ? (
            <>
              {titleParts.prefix}
              <span className="rounded-md bg-stroke-3/40 px-1.5 py-0.5 font-mono text-[13px] font-medium text-ink-secondary">
                {titleParts.tool}
              </span>
              {titleParts.suffix}
            </>
          ) : (
            permission.title
          )}
        </h3>
        {detail ? (
          // Read-only command/path readout — never an <input>/<textarea>
          // (WebView2 autofill painted a strange red cue over those).
          <pre
            className="mt-2 max-h-28 overflow-auto rounded-lg border border-stroke-3 bg-bg px-3 py-2 font-mono text-[12px] leading-relaxed text-ink-secondary [overflow-wrap:anywhere] whitespace-pre-wrap"
            tabIndex={-1}
            aria-label="Command details"
          >
            {detail}
          </pre>
        ) : null}

        {error ? (
          <div className="mt-2">
            <ErrorBanner message={error} />
          </div>
        ) : null}

        <div className="mt-3 flex items-center gap-1.5">
          {permission.options.includes("allow_once") ? (
            <Button
              size="sm"
              isLoading={isSubmitting}
              onClick={() => void handleDecision("allow_once")}
            >
              Allow once
            </Button>
          ) : null}
          {permission.options.includes("allow_always") ? (
            <Button
              size="sm"
              variant="secondary"
              isLoading={isSubmitting}
              onClick={() => void handleDecision("allow_always")}
            >
              Always allow
            </Button>
          ) : null}
          {permission.options.includes("deny") ? (
            <Button
              size="sm"
              variant="ghost"
              isLoading={isSubmitting}
              onClick={() => void handleDecision("deny")}
              className="ml-auto text-danger hover:bg-danger/10"
            >
              Deny
            </Button>
          ) : null}
        </div>
        </div>
      </div>
    </div>,
    document.body,
  )
}

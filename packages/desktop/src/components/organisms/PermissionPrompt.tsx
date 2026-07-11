import { useState } from "react"
import { X } from "lucide-react"
import { Button, IconButton } from "../atoms"
import { ErrorBanner } from "../molecules"
import { respondPermission, toInvokeError } from "../../lib/tauri"
import type { PendingPermission } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"

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
  const pushToast = useAppStore((s) => s.pushToast)
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const detail = formatDetail(permission.detail)
  const titleParts = splitTitle(permission.title)

  const handleDecision = async (decision: string) => {
    setError(null)
    setIsSubmitting(true)
    try {
      await respondPermission({
        sessionId: permission.sessionId,
        requestId: permission.requestId,
        decision,
      })
      setPendingPermission(null)
    } catch (err) {
      const message = toInvokeError(err)
      if (isStalePermissionError(message)) {
        // The engine-side request is gone (restart / session ended) — dismiss
        // instead of hard-blocking on an error that can never be resolved.
        setPendingPermission(null)
        pushToast("Permission request expired", "error")
      } else {
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

  return (
    <div
      role="dialog"
      aria-labelledby="permission-title"
      className="w-full max-w-[640px] animate-modal-in"
    >
      <div className="relative rounded-xl border border-stroke-3 bg-panel p-3 shadow-lg">
        <IconButton
          label="Dismiss"
          onClick={handleDismiss}
          className="absolute right-2 top-2 h-6 w-6"
        >
          <X className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
        <h3 id="permission-title" className="pr-6 text-sm font-semibold text-ink">
          {titleParts ? (
            <>
              {titleParts.prefix}
              <code className="rounded bg-fill-4 px-1 py-0.5 font-mono text-sm">
                {titleParts.tool}
              </code>
              {titleParts.suffix}
            </>
          ) : (
            permission.title
          )}
        </h3>
        {detail ? (
          <div className="relative mt-1.5 max-h-24 overflow-hidden rounded-md bg-fill-4">
            <p className="whitespace-pre-wrap break-words px-2.5 py-1.5 font-mono text-sm text-ink-secondary">
              {detail}
            </p>
            <div
              className="pointer-events-none absolute inset-x-0 bottom-0 h-4 bg-gradient-to-t from-fill-4 to-transparent"
              aria-hidden
            />
          </div>
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
  )
}

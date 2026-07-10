import { useState } from "react"
import { Button } from "../atoms"
import { ErrorBanner } from "../molecules"
import { respondPermission, toInvokeError } from "../../lib/tauri"
import type { PendingPermission } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"

type PermissionPromptProps = {
  permission: PendingPermission
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
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const detail = formatDetail(permission.detail)

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
      setError(toInvokeError(err))
    } finally {
      setIsSubmitting(false)
    }
  }

  return (
    <div
      role="dialog"
      aria-labelledby="permission-title"
      className="w-full max-w-lg animate-modal-in"
    >
      <div className="rounded-xl bg-panel p-3 shadow-lg">
        <h3 id="permission-title" className="text-sm font-semibold text-ink">
          {permission.title}
        </h3>
        {detail ? (
          <p className="mt-1.5 rounded-md bg-fill-4 px-2.5 py-1.5 font-mono text-sm text-ink-secondary">
            {detail}
          </p>
        ) : null}

        {error ? (
          <div className="mt-2">
            <ErrorBanner message={error} />
          </div>
        ) : null}

        <div className="mt-3 flex flex-wrap gap-1.5">
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
              variant="danger"
              isLoading={isSubmitting}
              onClick={() => void handleDecision("deny")}
            >
              Deny
            </Button>
          ) : null}
        </div>
      </div>
    </div>
  )
}

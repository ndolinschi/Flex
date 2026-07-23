import type { PendingPermission } from "../../lib/types"
import { cn } from "../../lib/utils"

type PermissionPromptProps = {
  permission: PendingPermission
}

const splitTitle = (
  title: string,
): { prefix: string; tool: string; suffix: string } | null => {
  const match = /^(.*?)`([^`]+)`(.*)$/.exec(title)
  if (!match) return null
  return { prefix: match[1], tool: match[2], suffix: match[3] }
}

export const formatPermissionDetail = (detail?: string): string | null => {
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
  }
  if (trimmed.startsWith("{") && trimmed.length > 120) {
    return `${trimmed.slice(0, 100)}…`
  }
  return trimmed
}

export const PermissionPrompt = ({ permission }: PermissionPromptProps) => {
  const detail = formatPermissionDetail(permission.detail)
  const titleParts = splitTitle(permission.title)

  return (
    <div
      role="dialog"
      aria-labelledby="permission-title"
      className="w-full animate-modal-in"
    >
      <div
        className={cn(
          "rounded-t-[var(--radius-composer)] border border-b-0 border-stroke-2",
          "bg-user-bubble px-3 pt-2.5 pb-2.5 shadow-[0_-4px_16px_-4px_var(--shadow-color)]",
        )}
      >
        <h3 id="permission-title" className="text-sm font-medium leading-snug text-ink">
          {titleParts ? (
            <>
              {titleParts.prefix}
              <span className="rounded-md bg-fill-3 px-1.5 py-0.5 font-mono text-xs font-medium text-ink-secondary">
                {titleParts.tool}
              </span>
              {titleParts.suffix}
            </>
          ) : (
            permission.title
          )}
        </h3>
        {detail ? (
          <pre
            className="mt-2 max-h-24 overflow-auto rounded-md border border-stroke-3 bg-fill-5 px-2.5 py-1.5 font-mono text-xs leading-relaxed text-ink-secondary [overflow-wrap:anywhere] whitespace-pre-wrap"
            tabIndex={-1}
            aria-label="Command details"
          >
            {detail}
          </pre>
        ) : null}
      </div>
    </div>
  )
}

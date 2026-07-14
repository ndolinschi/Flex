import type { PendingPermission } from "../../lib/types"
import { cn } from "../../lib/utils"

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
    // plain text
  }
  if (trimmed.startsWith("{") && trimmed.length > 120) {
    return `${trimmed.slice(0, 100)}…`
  }
  return trimmed
}

/** Docked permission header above the composer (QuestionPrompt seam).
 * Allow / Deny actions live in the composer footer via `PermissionActions`. */
export const PermissionPrompt = ({ permission }: PermissionPromptProps) => {
  const detail = formatPermissionDetail(permission.detail)
  const titleParts = splitTitle(permission.title)

  return (
    <div
      role="dialog"
      aria-labelledby="permission-title"
      className="w-full max-w-[var(--content-rail)] animate-modal-in"
    >
      <div
        className={cn(
          "rounded-t-[var(--radius-composer)] border border-b-0 border-stroke-3",
          "bg-user-bubble px-4 pt-3 pb-3.5 shadow-[0_-4px_16px_-4px_var(--shadow-color)]",
        )}
      >
        <h3 id="permission-title" className="text-sm font-semibold text-ink">
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
          <pre
            className="mt-2 max-h-28 overflow-auto rounded-lg border border-stroke-3 bg-bg px-3 py-2 font-mono text-[12px] leading-relaxed text-ink-secondary [overflow-wrap:anywhere] whitespace-pre-wrap"
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

import { useState, type KeyboardEvent, type MouseEvent } from "react"
import { Pencil, X } from "lucide-react"
import type { SessionMeta } from "../../lib/types"
import { sessionLabel } from "../../lib/types"
import { formatCompactTime, cn } from "../../lib/utils"
import { IconButton, RunningDot, TextInput } from "../atoms"

type SessionListItemProps = {
  session: SessionMeta
  isActive: boolean
  isRunning?: boolean
  onSelect: (id: string) => void
  onRename: (id: string, title: string) => Promise<void>
  onDelete: (id: string) => Promise<void>
}

export const SessionListItem = ({
  session,
  isActive,
  isRunning = false,
  onSelect,
  onRename,
  onDelete,
}: SessionListItemProps) => {
  const [isEditing, setIsEditing] = useState(false)
  const [draft, setDraft] = useState(session.title ?? "")
  const [isDeleting, setIsDeleting] = useState(false)

  const label = sessionLabel(session)

  const handleSaveRename = async () => {
    const trimmed = draft.trim()
    if (!trimmed) {
      setIsEditing(false)
      return
    }
    await onRename(session.id, trimmed)
    setIsEditing(false)
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault()
      void handleSaveRename()
    }
    if (e.key === "Escape") {
      setDraft(session.title ?? "")
      setIsEditing(false)
    }
  }

  const handleDelete = async (e: MouseEvent) => {
    e.stopPropagation()
    if (!window.confirm(`Delete "${label}"?`)) return
    setIsDeleting(true)
    try {
      await onDelete(session.id)
    } finally {
      setIsDeleting(false)
    }
  }

  return (
    <div
      role="button"
      tabIndex={0}
      aria-label={`Session ${label}`}
      aria-current={isActive ? "true" : undefined}
      onClick={() => {
        if (!isEditing) onSelect(session.id)
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" && !isEditing) onSelect(session.id)
      }}
      className={cn(
        "group flex min-h-7 items-center gap-3 rounded-sm px-2 py-1.5",
        "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
        isActive ? "bg-fill-5" : "hover:bg-fill-4",
      )}
    >
      <span className="flex min-w-0 flex-1 items-center gap-1.5">
        <span className="flex h-5 w-5 shrink-0 items-center justify-center">
          {isRunning ? <RunningDot /> : null}
        </span>

        {isEditing ? (
          <TextInput
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={() => void handleSaveRename()}
            onKeyDown={handleKeyDown}
            onClick={(e) => e.stopPropagation()}
            aria-label="Rename session"
            className="h-6 min-w-0 flex-1 text-sm"
          />
        ) : (
          <p
            className={cn(
              "min-w-0 flex-1 overflow-hidden whitespace-nowrap text-sm",
              "[mask-image:linear-gradient(to_right,#000_calc(100%-16px),transparent)]",
              isActive ? "text-ink" : "text-ink-secondary",
            )}
          >
            {label}
          </p>
        )}
      </span>

      {isEditing ? null : (
        <span className="flex shrink-0 items-center">
          <span
            className={cn(
              "shrink-0 text-xs tracking-[0.07px] [font-variant-numeric:tabular-nums]",
              "group-hover:hidden group-focus-within:hidden",
              isActive ? "text-ink-secondary" : "text-ink-muted",
            )}
          >
            {formatCompactTime(session.updated_at_ms)}
          </span>
          <span
            className={cn(
              "flex max-w-0 shrink-0 items-center overflow-hidden opacity-0",
              "pointer-events-none transition-[max-width,opacity] duration-[100ms] ease-[var(--easing-default)]",
              "group-hover:pointer-events-auto group-hover:max-w-[60px] group-hover:opacity-100",
              "group-focus-within:pointer-events-auto group-focus-within:max-w-[60px] group-focus-within:opacity-100",
            )}
          >
            <IconButton
              label="Rename session"
              className="h-6 w-6"
              onClick={(e) => {
                e.stopPropagation()
                setDraft(session.title ?? label)
                setIsEditing(true)
              }}
            >
              <Pencil className="h-3 w-3" aria-hidden />
            </IconButton>
            <IconButton
              label="Delete session"
              className="h-6 w-6"
              disabled={isDeleting}
              onClick={handleDelete}
            >
              <X className="h-3 w-3" aria-hidden />
            </IconButton>
          </span>
        </span>
      )}
    </div>
  )
}

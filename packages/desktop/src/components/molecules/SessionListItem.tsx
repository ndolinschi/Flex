import { useState, type KeyboardEvent, type MouseEvent } from "react"
import {
  ArchiveRestore,
  Archive as ArchiveIcon,
  Copy,
  MoreHorizontal,
  Pencil,
  Pin,
  Plus,
  Trash2,
  TriangleAlert,
} from "lucide-react"
import type { SessionMeta, WorkspaceStatusDto } from "../../lib/types"
import { sessionLabel } from "../../lib/types"
import { formatCompactTime, cn } from "../../lib/utils"
import { IconButton, RunningDot, TextInput, Tooltip } from "../atoms"
import { ContextMenu, type ContextMenuItem } from "./ContextMenu"

type SessionListItemProps = {
  session: SessionMeta
  isActive: boolean
  isRunning?: boolean
  /** Set when the last resume attempt for this session failed; shows a warning icon. */
  errorMessage?: string | null
  /** Workspace diff status for the subtitle line; undefined = loading, null = no isolated workspace. */
  workspaceStatus?: WorkspaceStatusDto | null
  /** Background-completed turn(s) not yet seen; shows a static accent dot
   * (isRunning takes precedence). When a positive count is passed, the title
   * is also prefixed "(N) " (reference design). SessionSidebar currently
   * passes a coerced boolean (`!!unreadBySession[id]`) — accepting both
   * shapes keeps this component working either way; see report for the
   * one-line change that unlocks the count prefix end-to-end. */
  unread?: boolean | number
  /** Whether this session is pinned (reference-design "Pinned" group). */
  pinned?: boolean
  /** Whether this session is archived (dimmed row, restore action instead of archive/delete). */
  archived?: boolean
  onSelect: (id: string) => void
  onRename: (id: string, title: string) => Promise<void>
  onDelete: (id: string) => Promise<void>
  /** "New agent" in this session's repo — used by the context menu only. */
  onNewAgentInRepo?: (cwd: string) => void
  onTogglePin?: (id: string) => void
  onSetArchived?: (id: string, archived: boolean) => void
}

/** Parsed `+N -M` diff counters, if the workspace summary is in that shape. */
const parseDiffStat = (
  summary: string,
): { added: number; removed: number } | null => {
  const match = summary.match(/\+(\d+)\D+-(\d+)/)
  if (!match) return null
  return { added: Number(match[1]), removed: Number(match[2]) }
}

export const SessionListItem = ({
  session,
  isActive,
  isRunning = false,
  errorMessage,
  workspaceStatus,
  unread = false,
  pinned = false,
  archived = false,
  onSelect,
  onRename,
  onDelete,
  onNewAgentInRepo,
  onTogglePin,
  onSetArchived,
}: SessionListItemProps) => {
  const [isEditing, setIsEditing] = useState(false)
  const [draft, setDraft] = useState(session.title ?? "")
  const [isDeleting, setIsDeleting] = useState(false)
  const [menuPosition, setMenuPosition] = useState<{ x: number; y: number } | null>(
    null,
  )

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

  const startRename = () => {
    setDraft(session.title ?? label)
    setIsEditing(true)
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

  const handleDelete = async (e?: MouseEvent) => {
    e?.stopPropagation()
    if (!window.confirm(`Delete "${label}"?`)) return
    setIsDeleting(true)
    try {
      await onDelete(session.id)
    } finally {
      setIsDeleting(false)
    }
  }

  const handleContextMenu = (e: MouseEvent) => {
    e.preventDefault()
    setMenuPosition({ x: e.clientX, y: e.clientY })
  }

  const contextMenuItems: ContextMenuItem[] = [
    {
      type: "item",
      label: "Rename",
      icon: Pencil,
      onSelect: startRename,
    },
    {
      type: "item",
      label: "Copy Path",
      icon: Copy,
      onSelect: () => {
        void navigator.clipboard.writeText(session.cwd)
      },
    },
    {
      type: "item",
      label: "New Agent in this repo",
      icon: Plus,
      disabled: !onNewAgentInRepo,
      onSelect: () => onNewAgentInRepo?.(session.cwd),
    },
    { type: "separator" },
    {
      type: "item",
      label: pinned ? "Unpin" : "Pin",
      icon: Pin,
      disabled: !onTogglePin,
      onSelect: () => onTogglePin?.(session.id),
    },
    {
      type: "item",
      label: archived ? "Restore" : "Archive",
      icon: archived ? ArchiveRestore : ArchiveIcon,
      disabled: !onSetArchived,
      onSelect: () => onSetArchived?.(session.id, !archived),
    },
    { type: "separator" },
    {
      type: "item",
      label: "Delete",
      icon: Trash2,
      danger: true,
      disabled: isDeleting,
      onSelect: () => void handleDelete(),
    },
  ]

  const diffStat = workspaceStatus ? parseDiffStat(workspaceStatus.summary) : null
  const showSubtitle = !isEditing && !!workspaceStatus
  // Numeric unread > 0 gets the "(N) " title prefix (reference design); a
  // plain `true` (current SessionSidebar pass-site) only shows the dot below.
  const unreadCount = typeof unread === "number" && unread > 0 ? unread : null

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
      onDoubleClick={() => {
        if (!isEditing) startRename()
      }}
      onContextMenu={handleContextMenu}
      className={cn(
        "group relative flex min-h-7 items-center gap-3 rounded-sm px-2 py-1.5",
        "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
        isActive ? "bg-fill-5" : "hover:bg-fill-4",
        archived && "opacity-60",
      )}
    >
      <span className="flex min-w-0 flex-1 flex-col justify-center gap-0.5">
        <span className="flex items-center gap-1.5">
          <span
            className={cn(
              "flex shrink-0 items-center justify-center",
              isRunning ? "h-5 w-5" : "h-3.5 w-3.5",
            )}
          >
            {isRunning ? (
              <RunningDot />
            ) : unread ? (
              <Tooltip label="Unread">
                <span
                  className="h-[5px] w-[5px] shrink-0 rounded-full bg-accent"
                  aria-hidden
                />
              </Tooltip>
            ) : null}
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
            <Tooltip label={label}>
              <p
                className={cn(
                  "min-w-0 flex-1 overflow-hidden whitespace-nowrap text-sm",
                  "[mask-image:linear-gradient(to_right,#000_calc(100%-16px),transparent)]",
                  isActive ? "text-ink" : "text-ink-secondary",
                )}
              >
                {unreadCount ? (
                  <span className="text-ink">({unreadCount}) </span>
                ) : null}
                {label}
              </p>
            </Tooltip>
          )}

          {!isEditing && errorMessage ? (
            <Tooltip label={errorMessage}>
              <span className="shrink-0" aria-label={`Resume failed: ${errorMessage}`}>
                <TriangleAlert className="h-3.5 w-3.5 text-yellow" aria-hidden />
              </span>
            </Tooltip>
          ) : null}
        </span>

        {showSubtitle ? (
          <span className="truncate pl-[26px] text-xs text-ink-muted">
            {diffStat ? (
              <>
                <span className="text-green">+{diffStat.added}</span>{" "}
                <span className="text-red">−{diffStat.removed}</span>
              </>
            ) : (
              `${workspaceStatus?.filesChanged ?? 0} files changed`
            )}
            {" · "}
            {formatCompactTime(session.updated_at_ms)}
          </span>
        ) : null}
      </span>

      {isEditing ? null : (
        <span className="flex shrink-0 items-center">
          {showSubtitle ? null : (
            <span
              className={cn(
                "shrink-0 text-xs tracking-[0.07px] [font-variant-numeric:tabular-nums]",
                "group-hover:hidden group-focus-within:hidden",
                isActive ? "text-ink-secondary" : "text-ink-muted",
              )}
            >
              {formatCompactTime(session.updated_at_ms)}
            </span>
          )}
          {/* Absolutely positioned (reference technique: .agent-sidebar-cell-trailing
              uses position:absolute + top:50%/translateY(-50%)) so the hover-actions'
              intrinsic button height can never inflate the row's own height. */}
          <span
            className={cn(
              "absolute right-2 top-1/2 flex max-w-0 -translate-y-1/2 items-center overflow-hidden opacity-0",
              "pointer-events-none transition-[max-width,opacity] duration-[100ms] ease-[var(--easing-default)]",
              "group-hover:pointer-events-auto group-hover:max-w-[90px] group-hover:opacity-100",
              "group-focus-within:pointer-events-auto group-focus-within:max-w-[90px] group-focus-within:opacity-100",
            )}
          >
            <Tooltip label={pinned ? "Unpin" : "Pin"}>
              <IconButton
                label={pinned ? "Unpin session" : "Pin session"}
                className="!h-6 !w-6"
                disabled={!onTogglePin}
                onClick={(e) => {
                  e.stopPropagation()
                  onTogglePin?.(session.id)
                }}
              >
                <Pin
                  className={cn("h-3 w-3", pinned && "fill-current text-accent")}
                  aria-hidden
                />
              </IconButton>
            </Tooltip>
            {archived ? (
              <Tooltip label="Restore">
                <IconButton
                  label="Restore session"
                  className="!h-6 !w-6"
                  disabled={!onSetArchived}
                  onClick={(e) => {
                    e.stopPropagation()
                    onSetArchived?.(session.id, false)
                  }}
                >
                  <ArchiveRestore className="h-3 w-3" aria-hidden />
                </IconButton>
              </Tooltip>
            ) : (
              <Tooltip label="Archive">
                <IconButton
                  label="Archive session"
                  className="!h-6 !w-6"
                  disabled={!onSetArchived}
                  onClick={(e) => {
                    e.stopPropagation()
                    onSetArchived?.(session.id, true)
                  }}
                >
                  <ArchiveIcon className="h-3 w-3" aria-hidden />
                </IconButton>
              </Tooltip>
            )}
            <IconButton
              label="More actions"
              className="!h-6 !w-6"
              onClick={(e) => {
                e.stopPropagation()
                const rect = e.currentTarget.getBoundingClientRect()
                setMenuPosition({ x: rect.left, y: rect.bottom })
              }}
            >
              <MoreHorizontal className="h-3 w-3" aria-hidden />
            </IconButton>
          </span>
        </span>
      )}

      <ContextMenu
        position={menuPosition}
        items={contextMenuItems}
        onClose={() => setMenuPosition(null)}
      />
    </div>
  )
}

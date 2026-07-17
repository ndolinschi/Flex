import { memo, useState, type KeyboardEvent, type MouseEvent } from "react"
import {
  ArchiveRestore,
  Archive as ArchiveIcon,
  Copy,
  Pencil,
  Pin,
  Plus,
  Trash2,
  TriangleAlert,
} from "@/components/icons"
import type { GitStatusSummary, SessionMeta, WorkspaceStatusDto } from "../../lib/types"
import { sessionLabel } from "../../lib/types"
import { formatCompactTime, cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import { RunningDot, TextInput, Tooltip } from "../atoms"
import { ConfirmDialog } from "./ConfirmDialog"
import { ContextMenu, type ContextMenuItem } from "./ContextMenu"
import { SessionRowActions } from "./SessionRowActions"
import {
  SessionRowSubtitle,
  sessionRowHasSubtitle,
} from "./SessionRowSubtitle"

type SessionListItemProps = {
  session: SessionMeta
  isActive: boolean
  /** Set when the last resume attempt for this session failed; shows a warning icon. */
  errorMessage?: string | null
  /** Workspace diff status for the subtitle line; undefined = loading, null = no isolated workspace. */
  workspaceStatus?: WorkspaceStatusDto | null
  /** Same `["git-status", cwd, sessionId]` summary the Changes tab reads —
   * used as the subtitle's change indicator for non-isolated sessions
   * (`workspaceStatus` above only ever resolves for isolated ones), so the
   * numbers always agree with the Changes tab. */
  gitStatus?: GitStatusSummary
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

export const SessionListItem = memo(function SessionListItem({
  session,
  isActive,
  errorMessage,
  workspaceStatus,
  gitStatus,
  pinned = false,
  archived = false,
  onSelect,
  onRename,
  onDelete,
  onNewAgentInRepo,
  onTogglePin,
  onSetArchived,
}: SessionListItemProps) {
  // Per-id selectors — parent must not pass the whole streaming/unread maps
  // or every row redraws on every other session's stream tick.
  const isRunning = useAppStore((s) => !!s.streamingSessions[session.id])
  const unread = useAppStore((s) => s.unreadBySession[session.id])
  const [isEditing, setIsEditing] = useState(false)
  const [draft, setDraft] = useState(session.title ?? "")
  const [isDeleting, setIsDeleting] = useState(false)
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false)
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

  const requestDelete = (e?: MouseEvent) => {
    e?.stopPropagation()
    setConfirmDeleteOpen(true)
  }

  const handleConfirmDelete = async () => {
    setIsDeleting(true)
    try {
      await onDelete(session.id)
      setConfirmDeleteOpen(false)
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
        void navigator.clipboard.writeText(session.base_cwd || session.cwd)
      },
    },
    {
      type: "item",
      label: "New Agent in this repo",
      icon: Plus,
      disabled: !onNewAgentInRepo,
      onSelect: () => onNewAgentInRepo?.(session.base_cwd || session.cwd),
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
      onSelect: () => requestDelete(),
    },
  ]

  const showSubtitle =
    !isEditing && sessionRowHasSubtitle(workspaceStatus, gitStatus)
  // Numeric unread > 0 gets the "(N) " title prefix (reference design).
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
        // Selected must read stronger than hover (Feel: Whisper fills) — fill-2 ≈8%, fill-4 ≈6%.
        isActive ? "bg-fill-2" : "hover:bg-fill-4",
        archived && "opacity-60",
      )}
    >
      <span className="flex min-w-0 flex-1 flex-col justify-center gap-0.5">
        <span className="flex items-center gap-1.5">
          <span
            className="flex h-5 w-5 shrink-0 items-center justify-center"
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
                  // Soft edge always; on hover/focus widen the dissolve so the
                  // title fades into shadow under the trailing action tray.
                  "[mask-image:linear-gradient(to_right,black_0%,black_calc(100%-12px),transparent_100%)]",
                  "group-hover:[mask-image:linear-gradient(to_right,black_0%,black_calc(100%-72px),transparent_100%)]",
                  "group-focus-within:[mask-image:linear-gradient(to_right,black_0%,black_calc(100%-72px),transparent_100%)]",
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
          <SessionRowSubtitle
            updatedAtMs={session.updated_at_ms}
            workspaceStatus={workspaceStatus}
            gitStatus={gitStatus}
          />
        ) : null}
      </span>

      {/* Surface scrub under trailing actions — long titles dissolve into the
          row chrome instead of sitting under the pin/archive/more icons. */}
      {isEditing ? null : (
        <span
          aria-hidden
          className={cn(
            "pointer-events-none absolute inset-y-0 right-0 z-[1] w-[5.75rem] rounded-r-sm",
            "opacity-0 transition-opacity duration-[var(--duration-fast)] ease-[var(--easing-default)]",
            "group-hover:opacity-100 group-focus-within:opacity-100",
            // Match the row fill (not solid panel) so the scrub doesn't flash a
            // different slab over hover/selected translucent fills.
            isActive ? "bg-fill-2" : "bg-fill-4",
            // Soft left edge so the title fades into shadow toward the buttons.
            "[mask-image:linear-gradient(to_left,black_0%,black_42%,transparent_100%)]",
          )}
        />
      )}

      {isEditing ? null : (
        <SessionRowActions
          pinned={pinned}
          archived={archived}
          showTrailingTime={!showSubtitle}
          updatedAtMs={session.updated_at_ms}
          isActive={isActive}
          formatTime={formatCompactTime}
          canTogglePin={!!onTogglePin}
          canSetArchived={!!onSetArchived}
          onTogglePin={() => onTogglePin?.(session.id)}
          onSetArchived={(_e, next) => onSetArchived?.(session.id, next)}
          onOpenMenu={(e) => {
            const rect = e.currentTarget.getBoundingClientRect()
            setMenuPosition({ x: rect.left, y: rect.bottom })
          }}
        />
      )}

      <ContextMenu
        position={menuPosition}
        items={contextMenuItems}
        onClose={() => setMenuPosition(null)}
      />

      <ConfirmDialog
        open={confirmDeleteOpen}
        title="Delete session"
        description={`Delete "${label}"? This cannot be undone.`}
        confirmLabel="Delete"
        danger
        isLoading={isDeleting}
        onConfirm={() => void handleConfirmDelete()}
        onCancel={() => setConfirmDeleteOpen(false)}
      />
    </div>
  )
})

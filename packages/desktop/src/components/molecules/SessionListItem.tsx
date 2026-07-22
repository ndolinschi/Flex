import { memo, useCallback, useState, type KeyboardEvent, type MouseEvent } from "react"
import {
  ArchiveRestore,
  Archive as ArchiveIcon,
  CircleAlert,
  Copy,
  MessageCircleQuestion,
  Pencil,
  Pin,
  Plus,
  Trash2,
  TriangleAlert,
} from "lucide-react"
import { useStoreWithEqualityFn } from "zustand/traditional"
import type { GitStatusSummary, SessionMeta, WorkspaceStatusDto } from "../../lib/types"
import { sessionLabel } from "../../lib/types"
import { basename, formatCompactTime, cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import { RunningDot, Tooltip } from "../atoms"
import { ConfirmDialog } from "./ConfirmDialog"
import { ContextMenu, type ContextMenuItem } from "./ContextMenu"
import { SessionRowActions } from "./SessionRowActions"
import {
  SessionRowSubtitle,
  sessionRowHasSubtitle,
} from "./SessionRowSubtitle"
import { Input } from "@/components/ui/input"

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
  // Single selector returning a tuple — one subscription instead of four,
  // with a field-level equality check so unrelated sessions' updates are skipped.
  const { isRunning, unread, needsInput, turnFailed } = useStoreWithEqualityFn(
    useAppStore,
    (s) => {
      const id = session.id
      return {
        isRunning: !!s.streamingSessions[id],
        unread: s.unreadBySession[id],
        needsInput:
          s.pendingPermission?.sessionId === id ||
          s.pendingQuestion?.sessionId === id,
        turnFailed: s.lastTurnSummary[id]?.stop_reason === "error",
      }
    },
    (a, b) =>
      a.isRunning === b.isRunning &&
      a.unread === b.unread &&
      a.needsInput === b.needsInput &&
      a.turnFailed === b.turnFailed,
  )
  const [isEditing, setIsEditing] = useState(false)
  const [draft, setDraft] = useState(session.title ?? "")
  const [isDeleting, setIsDeleting] = useState(false)
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false)
  const [menuPosition, setMenuPosition] = useState<{ x: number; y: number } | null>(
    null,
  )
  /** Defer icon action Buttons until first hover/focus — sticky thereafter. */
  const [actionsReady, setActionsReady] = useState(false)

  const label = sessionLabel(session)
  const repoLabel = pinned
    ? basename(session.base_cwd || session.cwd)
    : undefined

  const armActions = useCallback(() => {
    setActionsReady(true)
  }, [])

  const handleOpenMenu = useCallback((e: MouseEvent<HTMLButtonElement>) => {
    const rect = e.currentTarget.getBoundingClientRect()
    setMenuPosition({ x: rect.left, y: rect.bottom })
  }, [])

  const handleTogglePin = useCallback(() => {
    onTogglePin?.(session.id)
  }, [onTogglePin, session.id])

  const handleSetArchived = useCallback(
    (_e: MouseEvent, next: boolean) => {
      onSetArchived?.(session.id, next)
    },
    [onSetArchived, session.id],
  )

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
    !isEditing &&
    sessionRowHasSubtitle(workspaceStatus, gitStatus, repoLabel)
  // Numeric unread > 0 gets the "(N) " title prefix (reference design).
  const unreadCount = typeof unread === "number" && unread > 0 ? unread : null
  // Status-first triage: needs-input > working > failed > unread.
  const statusKind = needsInput
    ? "needs-input"
    : isRunning
      ? "working"
      : turnFailed
        ? "failed"
        : unread
          ? "unread"
          : null

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
      onPointerEnter={armActions}
      onFocusCapture={armActions}
      className={cn(
        // Cursor agent-sidebar-cell: padding 6×8, radius 6 (sm), gap 12.
        "group relative flex min-h-7 items-center gap-3 rounded-sm px-2 py-1.5",
        "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
        // Non-virtualized list: skip paint for off-screen rows in ScrollArea.
        // Safe here (fixed-ish row height); NEVER apply on virtualized timeline.
        "cv-auto-meta",
        // Selected stronger than hover (Feel: Whisper fills) — fill-2 ≈8%, fill-4 ≈6%.
        isActive ? "bg-fill-2" : "hover:bg-fill-4",
        archived && "opacity-60",
      )}
    >
      <span className="flex min-w-0 flex-1 flex-col justify-center gap-0.5">
        <span className="flex items-center gap-1.5">
          <span
            className="flex h-5 w-5 shrink-0 items-center justify-center"
          >
            {statusKind === "needs-input" ? (
              <Tooltip label="Needs your input">
                <MessageCircleQuestion
                  className="h-3.5 w-3.5 text-accent"
                  aria-label="Needs your input"
                />
              </Tooltip>
            ) : statusKind === "working" ? (
              <RunningDot />
            ) : statusKind === "failed" ? (
              <Tooltip label="Last turn failed">
                <CircleAlert
                  className="h-3.5 w-3.5 text-destructive"
                  aria-label="Last turn failed"
                />
              </Tooltip>
            ) : statusKind === "unread" ? (
              <Tooltip label="Unread">
                <span
                  className="h-[5px] w-[5px] shrink-0 rounded-full bg-accent"
                  aria-hidden
                />
              </Tooltip>
            ) : null}
          </span>

          {isEditing ? (
            <Input
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
                  // Plain ellipsis — no mask-image fade on hover (DESIGN: quiet chrome).
                  "min-w-0 flex-1 truncate text-left text-sm",
                  // Reserve room for absolute trailing actions so "…" stops before them.
                  "group-hover:pr-[5.5rem] group-focus-within:pr-[5.5rem]",
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
            repoLabel={repoLabel}
          />
        ) : null}
      </span>

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
          actionsReady={actionsReady}
          onTogglePin={handleTogglePin}
          onSetArchived={handleSetArchived}
          onOpenMenu={handleOpenMenu}
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

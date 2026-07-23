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
import { formatCompactTime, cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import { DiffStat, RunningDot, Tooltip } from "../atoms"
import { ConfirmDialog } from "./ConfirmDialog"
import { ContextMenu, type ContextMenuItem } from "./ContextMenu"
import { SessionRowActions } from "./SessionRowActions"
import { parseDiffStat } from "./SessionRowSubtitle"
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
  /**
   * Nest depth for child sessions (`parent_id`). 0 = root, 1 = nested under
   * parent (Cursor Agents nested thread indent).
   */
  nestDepth?: 0 | 1
  /** Role badge for nested subagents (e.g. `worker`, `searcher`). */
  roleLabel?: string | null
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
  nestDepth = 0,
  roleLabel = null,
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

  // Production Agents Web row is single-line h-8 with trailing +N −M that
  // fades on hover (actions take its place) — not a two-line subtitle stack.
  const workspaceDiff = workspaceStatus
    ? parseDiffStat(workspaceStatus.summary)
    : null
  const gitDiff =
    !workspaceStatus && gitStatus && gitStatus.totalCount > 0
      ? { added: gitStatus.totalAdded, removed: gitStatus.totalRemoved }
      : null
  const trailingDiff = workspaceDiff ?? gitDiff
  const hasTrailingDiff =
    !!trailingDiff && (trailingDiff.added > 0 || trailingDiff.removed > 0)

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
      tabIndex={isActive ? 0 : -1}
      data-session-row=""
      aria-label={`Session ${label}`}
      aria-current={isActive ? "true" : undefined}
      onClick={() => {
        if (!isEditing) onSelect(session.id)
      }}
      onKeyDown={(e) => {
        if (isEditing) return
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault()
          onSelect(session.id)
          return
        }
        const rows = Array.from(
          document.querySelectorAll<HTMLElement>("[data-session-row]"),
        )
        const index = rows.indexOf(e.currentTarget)
        if (index < 0) return
        const focusAt = (next: number) => {
          const row = rows[next]
          if (!row) return
          e.preventDefault()
          row.focus()
        }
        if (e.key === "ArrowDown") {
          focusAt(Math.min(index + 1, rows.length - 1))
        } else if (e.key === "ArrowUp") {
          focusAt(Math.max(index - 1, 0))
        } else if (e.key === "Home") {
          focusAt(0)
        } else if (e.key === "End") {
          focusAt(rows.length - 1)
        }
      }}
      onDoubleClick={() => {
        if (!isEditing) startRename()
      }}
      onContextMenu={handleContextMenu}
      onPointerEnter={armActions}
      onFocusCapture={armActions}
      className={cn(
        // Production agent-sidebar-cell: h-8 px-1.5 rounded-md, quaternary hover/selected.
        "agent-row group relative",
        isActive ? "agent-row-selected" : "agent-row-hover text-ink",
        "cv-auto-meta",
        nestDepth > 0 && "ml-4 border-l border-stroke-4 pl-2",
        archived && "opacity-60",
      )}
    >
      <div className="flex min-w-0 flex-1 items-center gap-2">
        {/* Production status slot ~13px wide */}
        <span className="flex w-[13px] shrink-0 items-center justify-center text-icon-2">
          {statusKind === "needs-input" ? (
            <Tooltip label="Needs your input">
              <MessageCircleQuestion
                className="size-3.5 text-accent"
                strokeWidth={1.5}
                aria-label="Needs your input"
              />
            </Tooltip>
          ) : statusKind === "working" ? (
            <RunningDot />
          ) : statusKind === "failed" ? (
            <Tooltip label="Last turn failed">
              <CircleAlert
                className="size-3.5 text-danger"
                strokeWidth={1.5}
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
                "min-w-0 flex-1 truncate text-left text-base",
                // Room for absolute trailing actions on hover.
                "group-hover:pr-[5.5rem] group-focus-within:pr-[5.5rem]",
                isActive ? "text-ink" : "text-ink-secondary",
              )}
            >
              {unreadCount ? (
                <span className="text-ink">({unreadCount}) </span>
              ) : null}
              {label}
              {roleLabel ? (
                <span className="ml-1.5 text-xs text-ink-faint">{roleLabel}</span>
              ) : null}
            </p>
          </Tooltip>
        )}

        {!isEditing && errorMessage ? (
          <Tooltip label={errorMessage}>
            <span className="shrink-0" aria-label={`Resume failed: ${errorMessage}`}>
              <TriangleAlert className="size-3.5 text-yellow" strokeWidth={1.5} aria-hidden />
            </span>
          </Tooltip>
        ) : null}
      </div>

      {/* In-flow trailing slot — same cross-axis as the title (not absolute
       * top-1/2 which floated +N−M above the baseline). Actions overlay. */}
      {!isEditing && hasTrailingDiff && trailingDiff ? (
        <DiffStat
          summary={trailingDiff}
          size="xs"
          className={cn(
            "shrink-0 leading-none",
            "transition-opacity duration-[var(--duration-fast)]",
            "group-hover:opacity-0 group-focus-within:opacity-0",
          )}
        />
      ) : null}

      {isEditing ? null : (
        <SessionRowActions
          pinned={pinned}
          archived={archived}
          showTrailingTime={!hasTrailingDiff}
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

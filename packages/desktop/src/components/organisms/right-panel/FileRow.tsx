import { memo, useCallback, useState, type MouseEvent as ReactMouseEvent } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { ChevronRight, Plus, Undo2 } from "lucide-react"
import { DiffStat, Spinner } from "../../atoms"
import { ConfirmDialog, DiffView } from "../../molecules"
import { Button } from "@/components/ui/button"
import { invalidateReviewQueries } from "../../../hooks/useWorkspaceActions"
import { buildPatch, type Hunk, type ParsedDiffFile } from "../../../lib/diff"
import {
  reviewApplyPatch,
  reviewFileDiff,
  reviewKeepFile,
  reviewUndoFile,
  toInvokeError,
} from "../../../lib/tauri"
import type { GitFileStatus } from "../../../lib/types"
import { useAppStore, sessionScopeKey } from "../../../stores/appStore"
import { basename, cn, fileIconForPath } from "../../../lib/utils"

export const STATUS_COLOR: Record<string, string> = {
  M: "text-yellow",
  A: "text-green",
  "?": "text-green",
  D: "text-red",
  R: "text-blue",
}

const PathPrefix = ({ dir }: { dir: string }) => {
  if (!dir) return null
  return (
    <span
      className="min-w-0 shrink overflow-hidden text-ellipsis whitespace-nowrap text-xs text-ink-faint"
      style={{ direction: "rtl" }}
    >
      <span style={{ direction: "ltr", unicodeBidi: "embed" }}>{dir}</span>
    </span>
  )
}

export const FileRow = memo(function FileRow({
  file,
  sessionId,
  isolated,
  expanded,
  onToggle,
  onError,
  selectable = false,
  selected = false,
  onToggleSelected,
}: {
  file: GitFileStatus
  sessionId: string | null
  isolated: boolean
  expanded: boolean
  onToggle: (path: string) => void
  onError: (message: string) => void
  selectable?: boolean
  selected?: boolean
  onToggleSelected?: (path: string) => void
}) {
  const isDir = file.path.endsWith("/")
  const dir = !isDir && file.path.includes("/")
    ? file.path.slice(0, file.path.lastIndexOf("/") + 1)
    : ""
  const name = isDir ? file.path : basename(file.path)
  const FileGlyph = fileIconForPath(file.path)
  const isNew = file.status === "A" || file.status === "?"
  const queryClient = useQueryClient()
  const [busyAction, setBusyAction] = useState<"keep" | "undo" | null>(null)
  const [confirmUndo, setConfirmUndo] = useState(false)
  const [actionsReady, setActionsReady] = useState(false)

  const {
    data: diff,
    isLoading,
    refetch: refetchDiff,
  } = useQuery({
    queryKey: ["review-file-diff", file.path, sessionId],
    queryFn: () => reviewFileDiff(sessionId!, file.path),
    enabled: expanded && !!sessionId,
    staleTime: 5_000,
  })

  const pushToast = useAppStore((s) => s.pushToast)
  const openWorkspaceFile = useAppStore((s) => s.openWorkspaceFile)
  const sessionKey = sessionId ? sessionScopeKey(sessionId) : ""

  const armActions = useCallback(() => {
    setActionsReady(true)
  }, [])

  const handleOpenFile = (e: ReactMouseEvent) => {
    e.stopPropagation()
    if (!sessionId || isDir || file.status === "D") return
    openWorkspaceFile(sessionKey, file.path)
  }

  const handleKeepFile = async (e: ReactMouseEvent) => {
    e.stopPropagation()
    if (!sessionId || busyAction) return
    if (!isolated) {
      onToggleSelected?.(file.path)
      return
    }
    setBusyAction("keep")
    try {
      await reviewKeepFile(sessionId, file.path)
      invalidateReviewQueries(queryClient, file.path, { sessionId })
      pushToast(`Kept ${name}`, "success")
    } catch (err) {
      const message = toInvokeError(err)
      pushToast(`Keep failed: ${message}`, "error")
      onError(message)
    } finally {
      setBusyAction(null)
    }
  }

  const runUndoFile = async () => {
    if (!sessionId || busyAction) return
    setBusyAction("undo")
    try {
      await reviewUndoFile(sessionId, file.path)
      invalidateReviewQueries(queryClient, file.path, { sessionId })
      pushToast(`Undid ${name}`, "success")
    } catch (err) {
      const message = toInvokeError(err)
      pushToast(`Undo failed: ${message}`, "error")
      onError(message)
    } finally {
      setBusyAction(null)
      setConfirmUndo(false)
    }
  }

  const handleKeepHunk = async (hunk: Hunk, diffFile: ParsedDiffFile) => {
    if (!sessionId || !isolated) return
    try {
      await reviewApplyPatch(
        sessionId,
        buildPatch(diffFile, [hunk]),
        "base",
        false,
      )
      invalidateReviewQueries(queryClient, file.path, { sessionId })
      void refetchDiff()
      pushToast(`Kept hunk in ${name}`, "success")
    } catch (err) {
      const message = toInvokeError(err)
      pushToast(`Keep failed: ${message}`, "error")
      onError(message)
    }
  }

  const handleUndoHunk = async (hunk: Hunk, diffFile: ParsedDiffFile) => {
    if (!sessionId) return
    try {
      await reviewApplyPatch(
        sessionId,
        buildPatch(diffFile, [hunk]),
        "worktree",
        true,
      )
      invalidateReviewQueries(queryClient, file.path, { sessionId })
      void refetchDiff()
      pushToast(`Undid hunk in ${name}`, "success")
    } catch (err) {
      const message = toInvokeError(err)
      pushToast(`Undo failed: ${message}`, "error")
      onError(message)
    }
  }

  return (
    <div className="pb-0.5">
      <div
        className={cn(
          "group relative flex h-7 w-full items-center gap-1.5 rounded-sm px-2.5",
          "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
          selectable && selected
            ? "bg-fill-2 hover:bg-fill-2"
            : expanded
              ? "bg-fill-5 hover:bg-fill-4"
              : "hover:bg-fill-4",
        )}
        onPointerEnter={armActions}
        onFocusCapture={armActions}
      >
        <Button
          variant="ghost"
          onClick={() => onToggle(file.path)}
          onDoubleClick={handleOpenFile}
          aria-expanded={expanded}
          className="h-auto min-w-0 flex-1 justify-start gap-1.5 px-0 py-0 font-normal hover:bg-transparent"
        >
          <FileGlyph
            className="h-3.5 w-3.5 shrink-0 text-ink-muted"
            aria-hidden
          />
          <span className="flex min-w-0 flex-1 items-center gap-0">
            <PathPrefix dir={dir} />
            <span className="min-w-0 truncate text-sm text-ink">{name}</span>
          </span>
        </Button>
        <DiffStat
          summary={{
            added: file.added ?? 0,
            removed: file.removed ?? 0,
          }}
          size="xs"
          className="shrink-0 justify-end"
        />
        {isNew ? (
          <span className="shrink-0 rounded-sm bg-green/15 px-1 text-[10px] font-medium text-green">
            New
          </span>
        ) : null}
        <ChevronRight
          className={cn(
            "h-3 w-3 shrink-0 text-icon-3",
            "transition-[transform,opacity] duration-[var(--duration-fast)]",
            expanded
              ? "rotate-90 opacity-100"
              : "opacity-0 group-hover:opacity-40",
          )}
          aria-hidden
          onClick={() => onToggle(file.path)}
        />
        {actionsReady ? (
          <span
            className={cn(
              "absolute right-1 top-1/2 flex -translate-y-1/2 items-center gap-0.5",
              "rounded-md bg-panel px-0.5 opacity-0 shadow-sm",
              "transition-opacity duration-[var(--duration-fast)]",
              "group-hover:opacity-100 focus-within:opacity-100",
            )}
          >
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label="Undo"
              title="Undo"
              onClick={(e) => {
                e.stopPropagation()
                setConfirmUndo(true)
              }}
              disabled={busyAction !== null}
              className="h-6 w-6 text-ink-muted hover:bg-fill-4 hover:text-ink"
            >
              {busyAction === "undo" ? (
                <Spinner size="sm" />
              ) : (
                <Undo2 className="h-3.5 w-3.5" aria-hidden />
              )}
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={isolated ? "Keep" : selected ? "Deselect" : "Select"}
              title={isolated ? "Keep" : selected ? "Deselect" : "Select"}
              onClick={(e) => void handleKeepFile(e)}
              disabled={busyAction !== null}
              className="h-6 w-6 text-ink-muted hover:bg-fill-4 hover:text-ink"
            >
              {busyAction === "keep" ? (
                <Spinner />
              ) : (
                <Plus className="h-3.5 w-3.5" aria-hidden />
              )}
            </Button>
          </span>
        ) : null}
      </div>

      {expanded ? (
        <div className="mx-2.5 mb-1 overflow-hidden rounded-[var(--radius-sm)] border border-stroke-4 bg-fill-5">
          {isLoading ? (
            <div className="flex items-center gap-2 px-3 py-2.5 text-sm text-ink-muted">
              <Spinner size="sm" /> Loading diff…
            </div>
          ) : (
            <DiffView
              diff={diff ?? ""}
              onKeepHunk={isolated ? handleKeepHunk : undefined}
              onUndoHunk={handleUndoHunk}
            />
          )}
        </div>
      ) : null}

      {confirmUndo ? (
        <ConfirmDialog
          open={confirmUndo}
          title={`Undo changes to ${name}?`}
          description="This reverts the file to its base state."
          confirmLabel="Undo"
          danger
          isLoading={busyAction === "undo"}
          onConfirm={() => void runUndoFile()}
          onCancel={() => setConfirmUndo(false)}
        />
      ) : null}
    </div>
  )
})

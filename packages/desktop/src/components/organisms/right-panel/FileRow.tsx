import { memo, useState, type MouseEvent as ReactMouseEvent } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { Check, ChevronRight, Undo2 } from "lucide-react"
import { IconButton, Spinner } from "../../atoms"
import { Collapsible, ConfirmDialog, DiffView } from "../../molecules"
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
import { useAppStore } from "../../../stores/appStore"
import { basename, cn, fileIconForPath } from "../../../lib/utils"

export const STATUS_COLOR: Record<string, string> = {
  M: "text-yellow",
  A: "text-green",
  "?": "text-green",
  D: "text-red",
  R: "text-blue",
}

/** Dir prefix of a file row's path, truncated from the LEFT when it doesn't
 * fit — the reference trick: the outer span is `direction: rtl` so ellipsis lands
 * on the left, but the text itself must stay logically LTR (a path, not
 * Arabic), so it's wrapped in an inner `direction: ltr` span. */
const PathPrefix = ({ dir }: { dir: string }) => {
  if (!dir) return null
  return (
    <span
      className="min-w-0 shrink overflow-hidden text-ellipsis whitespace-nowrap text-xs opacity-40"
      style={{ direction: "rtl" }}
    >
      <span style={{ direction: "ltr", unicodeBidi: "embed" }}>{dir}</span>
    </span>
  )
}

/** Memoized: a Changes tab with hundreds of files (e.g. after scaffolding a
 * project) re-renders the list on every 5s poll / selection toggle — without
 * memoization every row would re-render even though only one row's props
 * actually changed. Props are simple primitives/callbacks per row, so a
 * shallow prop comparison is enough (no custom comparator needed). */
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
  onToggle: () => void
  onError: (message: string) => void
  /** Show the commit-center checkbox (non-isolated sessions only). */
  selectable?: boolean
  selected?: boolean
  onToggleSelected?: () => void
}) {
  // A trailing-slash path is an untracked-directory porcelain entry (e.g.
  // "public/"), not a file — render it as just "public/" instead of
  // splitting it into a "public/" prefix + "public" basename, which would
  // duplicate the name (see `capture_session_baseline`'s "dir" sentinel).
  const isDir = file.path.endsWith("/")
  const dir = !isDir && file.path.includes("/")
    ? file.path.slice(0, file.path.lastIndexOf("/") + 1)
    : ""
  const name = isDir ? file.path : basename(file.path)
  const FileGlyph = fileIconForPath(file.path)
  const queryClient = useQueryClient()
  const [busyAction, setBusyAction] = useState<"keep" | "undo" | null>(null)
  const [confirmUndo, setConfirmUndo] = useState(false)

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

  const handleKeepFile = async (e: ReactMouseEvent) => {
    e.stopPropagation()
    if (!sessionId || busyAction) return
    setBusyAction("keep")
    try {
      await reviewKeepFile(sessionId, file.path)
      invalidateReviewQueries(queryClient, file.path)
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
      invalidateReviewQueries(queryClient, file.path)
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
      invalidateReviewQueries(queryClient, file.path)
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
      // Reverse-apply in the working dir — for non-isolated sessions
      // "worktree" still resolves to the repo cwd (review_apply_patch /
      // review_dirs in commands.rs: target "worktree" is always meta.cwd,
      // isolated or not), so this hunk-undo works the same either way.
      await reviewApplyPatch(
        sessionId,
        buildPatch(diffFile, [hunk]),
        "worktree",
        true,
      )
      invalidateReviewQueries(queryClient, file.path)
      void refetchDiff()
      pushToast(`Undid hunk in ${name}`, "success")
    } catch (err) {
      const message = toInvokeError(err)
      pushToast(`Undo failed: ${message}`, "error")
      onError(message)
    }
  }

  return (
    <li>
      <div
        className={cn(
          "group flex h-7 w-full items-center gap-1.5 rounded-[4px] px-3",
          "transition-colors duration-[var(--duration-fast)] hover:bg-fill-4",
        )}
      >
        {selectable ? (
          <input
            type="checkbox"
            checked={selected}
            onChange={() => onToggleSelected?.()}
            onClick={(e) => e.stopPropagation()}
            aria-label={`Include ${name} in commit`}
            className="h-3.5 w-3.5 shrink-0 accent-accent"
          />
        ) : null}
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={expanded}
          className="flex min-w-0 flex-1 items-center gap-2 text-left"
        >
          <FileGlyph
            className="h-3.5 w-3.5 shrink-0 text-ink-muted"
            aria-hidden
          />
          <span className="flex min-w-0 flex-1 items-center gap-0">
            <PathPrefix dir={dir} />
            <span className="shrink-0 truncate text-sm text-ink-secondary">
              {name}
            </span>
          </span>
        </button>
        <span className="flex shrink-0 items-center gap-1 text-sm [font-variant-numeric:tabular-nums]">
          {typeof file.added === "number" ? (
            <span className="text-green">+{file.added}</span>
          ) : null}
          {typeof file.removed === "number" && file.removed > 0 ? (
            <span className="text-red">-{file.removed}</span>
          ) : null}
          <span
            className={cn(
              "w-3 text-center font-mono text-[11px]",
              STATUS_COLOR[file.status] ?? "text-ink-muted",
            )}
            title={
              file.status === "?"
                ? "Untracked"
                : file.status === "M"
                  ? "Modified"
                  : file.status === "A"
                    ? "Added"
                    : file.status === "D"
                      ? "Deleted"
                      : file.status === "R"
                        ? "Renamed"
                        : file.status
            }
          >
            {file.status === "?" ? "U" : file.status}
          </span>
        </span>
        {/* Per-file quick actions — hidden until row hover, gap 4px per spec.
            Keep is meaningless without a base repo (non-isolated sessions
            have nowhere to "keep" into), so it's isolated-only. */}
        <span className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity duration-[var(--duration-fast)] group-hover:opacity-100">
          {isolated ? (
            <IconButton
              label="Keep"
              onClick={handleKeepFile}
              disabled={busyAction !== null}
              className="h-6 w-6"
            >
              {busyAction === "keep" ? (
                <Spinner size="sm" />
              ) : (
                <Check className="h-3.5 w-3.5" aria-hidden />
              )}
            </IconButton>
          ) : null}
          <IconButton
            label="Undo"
            onClick={(e) => {
              e.stopPropagation()
              setConfirmUndo(true)
            }}
            disabled={busyAction !== null}
            className="h-6 w-6"
          >
            {busyAction === "undo" ? (
              <Spinner size="sm" />
            ) : (
              <Undo2 className="h-3.5 w-3.5" aria-hidden />
            )}
          </IconButton>
        </span>
        <ChevronRight
          className={cn(
            "h-2.5 w-2.5 shrink-0 text-icon-3",
            "transition-[transform,opacity] duration-[var(--duration-fast)]",
            expanded
              ? "rotate-90 opacity-100"
              : "opacity-0 group-hover:opacity-100",
          )}
          aria-hidden
          onClick={onToggle}
        />
      </div>
      <Collapsible open={expanded}>
        <div className="border-y border-stroke-4 bg-fill-5 py-1">
          {isLoading ? (
            <div className="flex items-center gap-2 px-3 py-2 text-sm text-ink-muted">
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
      </Collapsible>

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
    </li>
  )
})


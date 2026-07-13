import { useEffect, useMemo, useRef, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { GitMerge, RefreshCw, XCircle } from "lucide-react"
import { Button, DiffStat, IconButton, ScrollArea } from "../../atoms"
import { ConfirmDialog } from "../../molecules"
import { useWorkspaceActions } from "../../../hooks/useWorkspaceActions"
import { useIsGitRepo } from "../../../hooks/useIsGitRepo"
import {
  gitBranch,
  gitStatusSinceBaseline,
  isIsolated,
} from "../../../lib/tauri"
import { invalidateGitQueries } from "../../../lib/invalidateGitQueries"
import type { SessionMeta } from "../../../lib/types"
import { useAppStore } from "../../../stores/appStore"
import { cn } from "../../../lib/utils"
import { CommitCenter } from "./CommitCenter"
import { FileRow } from "./FileRow"

export const ChangesTab = ({ active }: { active: SessionMeta | undefined }) => {
  const cwd = active?.cwd ?? ""
  const sessionId = active?.id ?? null
  const [expandedPath, setExpandedPath] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [confirmDiscard, setConfirmDiscard] = useState(false)
  const queryClient = useQueryClient()

  // Gate session-scoped git queries/actions on the cwd actually being a git
  // repo — defaults to `true` while loading/no cwd so this never flashes the
  // "not a git repository" empty state for an instant on a real repo before
  // the query resolves (mirrors ContextBar's `isRepo` gating).
  //
  // Re-check aggressively: `git init` in the Terminal (or mid-session) must
  // not leave a sticky `false` that permanently disables the status query.
  // Nested repos under this cwd are intentionally NOT auto-detected — the
  // session's own cwd is the product boundary.
  const {
    data: isRepo = true,
    isFetching: isRepoFetching,
    refetch: refetchIsRepo,
  } = useIsGitRepo(cwd || undefined)

  const { data: summary, refetch, isFetching } = useQuery({
    queryKey: ["git-status", cwd, sessionId],
    queryFn: () => gitStatusSinceBaseline(sessionId!),
    enabled: !!cwd && !!sessionId && isRepo,
    refetchInterval: 5_000,
    refetchOnWindowFocus: true,
  })

  const handleRefresh = () => {
    invalidateGitQueries(queryClient)
    void refetchIsRepo()
    if (isRepo) void refetch()
  }

  // `files` is the server-capped row list (see `MAX_STATUS_FILES` in
  // commands.rs) — a session with hundreds of changed files (e.g. after
  // scaffolding a project) never asks this list to render every row.
  // `totalCount`/totals reflect the full, untruncated set so the header
  // count and +/- badge stay accurate past the cap.
  const files = summary?.files ?? []
  const totalCount = summary?.totalCount ?? 0
  const truncated = summary?.truncated ?? false

  // Commit-center selection (spec #48): which files are staged when the
  // split-button commit runs. Defaults to "all selected" and reconciles
  // against the live file list on every change — new files default to
  // selected, files that disappeared (committed/undone elsewhere) are
  // dropped so a stale path never gets passed to a commit command.
  const [selected, setSelected] = useState<Set<string>>(
    () => new Set(files.map((f) => f.path)),
  )
  useEffect(() => {
    setSelected((prev) => {
      const paths = files.map((f) => f.path)
      const pathSet = new Set(paths)
      const next = new Set([...prev].filter((p) => pathSet.has(p)))
      for (const p of paths) {
        if (!prev.has(p) && !next.has(p)) next.add(p)
      }
      return next
    })
  }, [files])

  const { data: branch } = useQuery({
    queryKey: ["git-branch", cwd],
    queryFn: () => gitBranch(cwd),
    enabled: !!cwd && isRepo,
    staleTime: 10_000,
  })

  const { data: isolated = false } = useQuery({
    queryKey: ["is-isolated", sessionId],
    queryFn: () => isIsolated(sessionId!),
    enabled: !!sessionId,
    staleTime: 5_000,
  })

  const workspace = useWorkspaceActions(sessionId, setError)

  // Agent turns usually touch files — refresh when this session stops streaming.
  const isStreaming = useAppStore((s) =>
    sessionId ? !!s.streamingSessions[sessionId] : false,
  )
  const prevStreaming = useRef(isStreaming)
  useEffect(() => {
    if (prevStreaming.current && !isStreaming) {
      invalidateGitQueries(queryClient)
    }
    prevStreaming.current = isStreaming
  }, [isStreaming, queryClient])

  const totals = useMemo(
    () => ({
      added: summary?.totalAdded ?? 0,
      removed: summary?.totalRemoved ?? 0,
    }),
    [summary],
  )

  // design: the aggregate bar's buttons read "Keep All"/"Undo All" when
  // multiple files are pending, singular "Keep"/"Undo" for exactly one — one
  // bar, label swaps, not separate components.
  const aggregateSuffix = totalCount === 1 ? "" : " All"

  if (!active) {
    return (
      <div className="flex flex-1 items-center justify-center px-4 text-center">
        <p className="text-sm text-ink-muted">No active session.</p>
      </div>
    )
  }

  // No git repo in this cwd at all — calm empty state, not an error. A repo
  // with an unborn HEAD (no commits yet) is still `isRepo === true` and
  // keeps the regular UI below (see `git_is_repo`'s doc comment). Refresh
  // re-runs `git_is_repo` so a just-ran `git init` in this session cwd is
  // picked up without leaving and re-entering the tab.
  if (!isRepo) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 px-4 text-center">
        <p className="text-sm text-ink-muted">Not a git repository.</p>
        <IconButton
          label="Refresh changes"
          onClick={handleRefresh}
          className="h-7 w-7"
        >
          <RefreshCw
            className={cn("h-3.5 w-3.5", isRepoFetching && "animate-spin")}
            aria-hidden
          />
        </IconButton>
      </div>
    )
  }

  return (
    <>
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-stroke-3 px-3 text-sm [font-variant-numeric:tabular-nums]">
        <span className="min-w-0 truncate text-ink-secondary">
          {totalCount === 0
            ? "No changes"
            : `${totalCount} file${totalCount === 1 ? "" : "s"} changed`}
        </span>
        <span className="ml-auto flex shrink-0 items-center gap-1.5">
          <DiffStat summary={totals} size="sm" />
          <IconButton
            label="Refresh changes"
            onClick={handleRefresh}
            className="h-6 w-6"
          >
            <RefreshCw
              className={cn(
                "h-3 w-3",
                (isFetching || isRepoFetching) && "animate-spin",
              )}
              aria-hidden
            />
          </IconButton>
        </span>
      </div>

      {error ? (
        <p className="border-b border-stroke-3 bg-danger-subtle px-3 py-1.5 text-xs text-danger">
          {error}
        </p>
      ) : null}

      {/* Select-all header — commit center is non-isolated only (isolated
          sessions commit via integrate instead), so the checkbox column
          only appears there. */}
      {!isolated && files.length > 0 ? (
        <div className="flex h-7 shrink-0 items-center gap-2 border-b border-stroke-3 px-3 text-xs text-ink-muted">
          <input
            type="checkbox"
            checked={selected.size === files.length}
            ref={(el) => {
              if (el) {
                el.indeterminate =
                  selected.size > 0 && selected.size < files.length
              }
            }}
            onChange={() =>
              setSelected((prev) =>
                prev.size === files.length
                  ? new Set()
                  : new Set(files.map((f) => f.path)),
              )
            }
            aria-label="Select all files"
            className="h-3.5 w-3.5 shrink-0 accent-accent"
          />
          <span>
            {selected.size} of {files.length} selected
          </span>
        </div>
      ) : null}

      <ScrollArea className="min-h-0 flex-1">
        {totalCount === 0 ? (
          <p className="px-4 py-8 text-center text-sm text-ink-muted">
            No changes{branch ? ` in ${branch}` : ""}.
          </p>
        ) : (
          <ul className="py-1">
            {files.map((file) => (
              <FileRow
                key={file.path}
                file={file}
                sessionId={sessionId}
                isolated={isolated}
                expanded={expandedPath === file.path}
                onToggle={() =>
                  setExpandedPath((prev) =>
                    prev === file.path ? null : file.path,
                  )
                }
                onError={setError}
                selectable={!isolated}
                selected={selected.has(file.path)}
                onToggleSelected={() =>
                  setSelected((prev) => {
                    const next = new Set(prev)
                    if (next.has(file.path)) next.delete(file.path)
                    else next.add(file.path)
                    return next
                  })
                }
              />
            ))}
          </ul>
        )}
        {/* Server-side cap (MAX_STATUS_FILES in commands.rs) — a session with
            hundreds of changed files still only mounts `files.length` rows;
            this tells the user more exist without rendering them. */}
        {truncated ? (
          <p className="px-3 py-2 text-center text-xs text-ink-faint">
            +{totalCount - files.length} more files not shown
          </p>
        ) : null}
      </ScrollArea>

      {!isolated && sessionId ? (
        <CommitCenter
          sessionId={sessionId}
          cwd={cwd}
          selectedPaths={[...selected]}
          totalFiles={files.length}
          onError={setError}
        />
      ) : null}

      {isolated && files.length > 0 ? (
        <div className="flex shrink-0 items-center gap-1.5 border-t border-stroke-3 px-3 py-2">
          <Button
            variant="secondary"
            size="sm"
            isLoading={workspace.busy}
            onClick={() => void workspace.integrate()}
          >
            <GitMerge className="h-3 w-3" aria-hidden /> Keep{aggregateSuffix}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={workspace.busy}
            onClick={() => setConfirmDiscard(true)}
          >
            <XCircle className="h-3 w-3" aria-hidden /> Undo{aggregateSuffix}
          </Button>
        </div>
      ) : null}

      <ConfirmDialog
        open={confirmDiscard}
        title={`Undo${aggregateSuffix === "" ? "" : " all"} changes?`}
        description={
          files.length === 1
            ? "This discards the isolated workspace's change and reverts the file to its base state."
            : "This discards the isolated workspace and reverts every changed file to its base state."
        }
        confirmLabel={`Undo${aggregateSuffix}`}
        danger
        isLoading={workspace.busy}
        onConfirm={() => {
          void workspace.discard()
          setConfirmDiscard(false)
        }}
        onCancel={() => setConfirmDiscard(false)}
      />
    </>
  )
}

/* ── Panel shell ──────────────────────────────────────────────────────── */

/** right panel: Plan / Changes tabs, resizable via a left sash.
 *
 * Below the "narrow" viewport breakpoint (~940px, see hooks/useViewportWidth)
 * the panel switches from a side-by-side flex column to an absolutely
 * positioned overlay anchored to the right edge of the chat area, with a
 * dim backdrop that closes it on click — same width clamp, same open/close
 * state (`rightPanelOpen` semantics are unchanged, this is presentational
 * only). At "wide" it renders exactly as before. */

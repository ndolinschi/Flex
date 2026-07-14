import { useEffect, useMemo, useRef, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { GitMerge, RefreshCw, XCircle } from "lucide-react"
import { Button, Checkbox, DiffStat, IconButton, ScrollArea } from "../../atoms"
import { BranchPrStatusChip, ConfirmDialog, CreatePrDialog } from "../../molecules"
import { useWorkspaceActions } from "../../../hooks/useWorkspaceActions"
import { useIsGitRepo } from "../../../hooks/useIsGitRepo"
import {
  gitBranch,
  gitCreatePrForBranch,
  gitHasRemote,
  gitPrDraft,
  gitPrStatus,
  gitStatusSinceBaseline,
  isIsolated,
  toInvokeError,
} from "../../../lib/tauri"
import { toastPrOutcome } from "../../../lib/prOutcomeToast"
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

  const { data: hasRemote = false } = useQuery({
    queryKey: ["git-has-remote", cwd],
    queryFn: () => gitHasRemote(cwd),
    enabled: !!cwd && isRepo,
    staleTime: 10_000,
  })

  const { data: prStatus } = useQuery({
    queryKey: ["git-pr-status", cwd],
    queryFn: () => gitPrStatus(cwd),
    enabled: !!cwd && isRepo && hasRemote,
    staleTime: 30_000,
    refetchInterval: 60_000,
    refetchOnWindowFocus: true,
  })
  const branchPr = prStatus?.pr ?? null
  const [createPrOpen, setCreatePrOpen] = useState(false)
  const [creatingPr, setCreatingPr] = useState(false)
  const pushToast = useAppStore((s) => s.pushToast)

  const { data: prDraft } = useQuery({
    queryKey: ["git-pr-draft", cwd],
    queryFn: () => gitPrDraft(cwd),
    enabled: createPrOpen && !!cwd && isRepo,
    staleTime: 5_000,
  })

  const handleCreatePr = async (title: string, body: string) => {
    if (!cwd || creatingPr) return
    setCreatingPr(true)
    try {
      const outcome = await gitCreatePrForBranch(cwd, title, body)
      invalidateGitQueries(queryClient)
      toastPrOutcome(pushToast, outcome)
      setCreatePrOpen(false)
    } catch (err) {
      const msg = toInvokeError(err)
      pushToast(`Couldn't create PR: ${msg}`, "error")
      setError(msg)
    } finally {
      setCreatingPr(false)
    }
  }

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
        <GitMerge className="h-7 w-7 text-ink-faint opacity-70" aria-hidden />
        <p className="text-sm text-ink-secondary">Not a git repository</p>
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

  const showSelectAll = !isolated && files.length > 0
  const allSelected = showSelectAll && selected.size === files.length
  const someSelected = showSelectAll && selected.size > 0 && selected.size < files.length
  const headline =
    totalCount === 0
      ? "No changes"
      : `${totalCount} file${totalCount === 1 ? "" : "s"} changed`

  return (
    <>
      {/* Quiet chrome row — title / branch / PR / diffstat / refresh.
          Selection lives on a dedicated toolbar below so the header stays
          balanced with Plan / Files / Terminal. */}
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-2 border-b border-stroke-3 px-2.5 [font-variant-numeric:tabular-nums]">
        <div className="min-w-0 flex-1 truncate">
          <span className="text-sm text-ink">{headline}</span>
          {branch ? (
            <span className="text-sm text-ink-faint"> · {branch}</span>
          ) : null}
        </div>
        {branchPr ? (
          <BranchPrStatusChip pr={branchPr} />
        ) : hasRemote && prStatus?.ghAvailable ? (
          <Button
            variant="ghost"
            size="sm"
            className="h-6 shrink-0 px-1.5 text-xs"
            onClick={() => setCreatePrOpen(true)}
          >
            Create PR
          </Button>
        ) : null}
        {totalCount > 0 ? <DiffStat summary={totals} size="sm" /> : null}
        <IconButton
          label="Refresh changes"
          onClick={handleRefresh}
          className="h-6 w-6"
        >
          <RefreshCw
            className={cn(
              "h-3.5 w-3.5",
              (isFetching || isRepoFetching) && "animate-spin",
            )}
            aria-hidden
          />
        </IconButton>
      </div>

      {showSelectAll ? (
        <div className="flex h-7 shrink-0 items-center gap-2 border-b border-stroke-3 px-2.5">
          <Checkbox
            checked={allSelected}
            indeterminate={someSelected}
            onChange={() =>
              setSelected(
                allSelected
                  ? new Set()
                  : new Set(files.map((f) => f.path)),
              )
            }
            label={
              allSelected
                ? "Deselect all files"
                : `Select all ${files.length} files`
            }
          />
          <span className="min-w-0 flex-1 truncate text-xs text-ink-muted">
            {allSelected
              ? "All selected"
              : someSelected
                ? `${selected.size} of ${files.length} selected`
                : "Select files to commit"}
          </span>
        </div>
      ) : null}

      {error ? (
        <p className="border-b border-stroke-3 bg-danger-subtle px-2.5 py-1.5 text-xs text-danger">
          {error}
        </p>
      ) : null}

      <ScrollArea className="min-h-0 flex-1">
        {totalCount === 0 ? (
          <div className="flex flex-col items-center justify-center gap-2 px-4 py-12 text-center">
            <GitMerge
              className="h-7 w-7 text-ink-faint opacity-70"
              aria-hidden
            />
            <p className="text-sm text-ink-secondary">
              Working tree clean
              {branch ? (
                <span className="text-ink-faint"> on {branch}</span>
              ) : null}
            </p>
          </div>
        ) : (
          <ul className="flex flex-col gap-0.5 px-2 py-1.5">
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
          <p className="px-2.5 py-2 text-center text-xs text-ink-faint">
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
        <div className="flex shrink-0 items-center justify-end gap-2 border-t border-stroke-3 px-2.5 py-2.5">
          <Button
            variant="ghost"
            size="sm"
            disabled={workspace.busy}
            onClick={() => setConfirmDiscard(true)}
          >
            <XCircle className="h-3 w-3" aria-hidden /> Undo{aggregateSuffix}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            isLoading={workspace.busy}
            onClick={() => void workspace.integrate()}
          >
            <GitMerge className="h-3 w-3" aria-hidden /> Keep{aggregateSuffix}
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

      <CreatePrDialog
        open={createPrOpen}
        initialTitle={prDraft?.title ?? ""}
        initialBody={prDraft?.body ?? ""}
        isLoading={creatingPr}
        onCancel={() => {
          if (!creatingPr) setCreatePrOpen(false)
        }}
        onConfirm={(title, body) => {
          void handleCreatePr(title, body)
        }}
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

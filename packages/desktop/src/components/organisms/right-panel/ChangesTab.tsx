import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useVirtualizer } from "@tanstack/react-virtual"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { ChevronDown, GitMerge, PlusSquare, RefreshCw, Undo2, XCircle } from "lucide-react"
import { DiffStat } from "../../atoms"
import {
  BranchPrStatusChip,
  ConfirmDialog,
  CreatePrDialog,
  EmptyState,
  ErrorBanner,
  PanelToolbar,
  ToolQueryError,
  panelChromeIconClass,
} from "../../molecules"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
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
  reviewUndoFile,
  toInvokeError,
} from "../../../lib/tauri"
import { toastPrOutcome } from "../../../lib/prOutcomeToast"
import { invalidateGitQueries } from "../../../lib/invalidateGitQueries"
import { changesStatusRefetchInterval } from "../../../hooks/statusPoll"
import type { SessionMeta } from "../../../lib/types"
import { useAppStore } from "../../../stores/appStore"
import { cn } from "../../../lib/utils"
import { CommitCenter } from "./CommitCenter"
import { FileRow } from "./FileRow"

/** Collapsed file row = h-7 + 2px gap (padding). */
const ROW_ESTIMATE_PX = 30
/** Rough open-diff estimate until measureElement runs. */
const EXPANDED_ESTIMATE_PX = 220

export const ChangesTab = ({ active }: { active: SessionMeta | undefined }) => {
  const cwd = active?.cwd ?? ""
  const sessionId = active?.id ?? null
  const [expandedPath, setExpandedPath] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [confirmDiscard, setConfirmDiscard] = useState(false)
  const queryClient = useQueryClient()
  const commitCenterRef = useRef<HTMLDivElement>(null)
  const listRef = useRef<HTMLDivElement>(null)

  const {
    data: isRepo = true,
    isFetching: isRepoFetching,
    refetch: refetchIsRepo,
  } = useIsGitRepo(cwd || undefined)

  const isStreaming = useAppStore((s) =>
    sessionId ? !!s.streamingSessions[sessionId] : false,
  )

  const {
    data: summary,
    refetch,
    isFetching,
    isError: statusIsError,
    error: statusError,
    isPending: statusPending,
  } = useQuery({
    queryKey: ["git-status", cwd, sessionId],
    queryFn: () => gitStatusSinceBaseline(sessionId!),
    enabled: !!cwd && !!sessionId && isRepo,
    // Steady 30s poll (no aggressive 5s stream poll). Live updates come from
    // scoped invalidation on turn complete / FS-mutating tools.
    refetchInterval: changesStatusRefetchInterval(isStreaming),
    refetchOnWindowFocus: true,
  })

  const gitScope = useMemo(
    () => ({
      cwd: cwd || undefined,
      sessionId: sessionId ?? undefined,
    }),
    [cwd, sessionId],
  )

  const handleRefresh = () => {
    invalidateGitQueries(queryClient, gitScope)
    void refetchIsRepo()
    if (isRepo) void refetch()
  }

  const files = summary?.files ?? []
  const totalCount = summary?.totalCount ?? 0
  const truncated = summary?.truncated ?? false

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

  // Dense collapsed list by default (Cursor Changes); clear stale expansion.
  useEffect(() => {
    if (files.length === 0) {
      setExpandedPath(null)
      return
    }
    setExpandedPath((prev) =>
      prev && files.some((f) => f.path === prev) ? prev : null,
    )
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
    staleTime: 60_000,
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
      invalidateGitQueries(queryClient, gitScope)
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

  const prevStreaming = useRef(isStreaming)
  useEffect(() => {
    if (prevStreaming.current && !isStreaming) {
      invalidateGitQueries(queryClient, gitScope)
    }
    prevStreaming.current = isStreaming
  }, [isStreaming, queryClient, gitScope])

  const totals = useMemo(
    () => ({
      added: summary?.totalAdded ?? 0,
      removed: summary?.totalRemoved ?? 0,
    }),
    [summary],
  )

  const selectedPaths = useMemo(() => [...selected], [selected])

  const toggleExpand = useCallback((path: string) => {
    setExpandedPath((prev) => (prev === path ? null : path))
  }, [])

  const toggleSelected = useCallback((path: string) => {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  }, [])

  const selectAll = useCallback(() => {
    setSelected(new Set(files.map((f) => f.path)))
  }, [files])

  const [discardBusy, setDiscardBusy] = useState(false)

  const handleDiscardAll = async () => {
    if (!sessionId || discardBusy) return
    if (isolated) {
      void workspace.discard()
      setConfirmDiscard(false)
      return
    }
    setDiscardBusy(true)
    try {
      for (const file of files) {
        try {
          await reviewUndoFile(sessionId, file.path)
        } catch {
          /* continue remaining */
        }
      }
      invalidateGitQueries(queryClient, gitScope)
      pushToast("Discarded changes", "success")
      setConfirmDiscard(false)
    } catch (err) {
      setError(toInvokeError(err))
    } finally {
      setDiscardBusy(false)
    }
  }

  const virtualizer = useVirtualizer({
    count: files.length,
    getScrollElement: () => listRef.current,
    estimateSize: (index) =>
      expandedPath === files[index]?.path
        ? EXPANDED_ESTIMATE_PX
        : ROW_ESTIMATE_PX,
    overscan: 10,
    getItemKey: (index) => files[index]?.path ?? index,
    measureElement: (element) => (element as HTMLElement).offsetHeight,
    useAnimationFrameWithResizeObserver: true,
  })

  const aggregateSuffix = totalCount === 1 ? "" : " All"

  if (!active) {
    return (
      <EmptyState
        className="px-2.5"
        title="No active session"
        description="Select a session to review working-tree changes."
      />
    )
  }

  if (!isRepo) {
    return (
      <EmptyState
        className="px-2.5"
        icon={<GitMerge className="h-6 w-6" aria-hidden />}
        title="Not a git repository"
        description="Initialize a git repo in this project to track changes."
        action={
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label="Refresh changes"
            title="Refresh changes"
            onClick={handleRefresh}
            className={cn(
              "text-ink-muted hover:bg-fill-4 hover:text-ink",
              "h-6 w-6",
            )}
          >
            <RefreshCw
              className={cn("h-3.5 w-3.5", isRepoFetching && "animate-spin")}
              aria-hidden
            />
          </Button>
        }
      />
    )
  }

  const virtualItems = virtualizer.getVirtualItems()
  const listHeight = virtualizer.getTotalSize()
  const scrollToCommit = () => {
    commitCenterRef.current?.scrollIntoView({
      behavior: "smooth",
      block: "nearest",
    })
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Host chrome: Local | branch + inverse Create Branch & Commit pill (Cursor). */}
      <PanelToolbar
        aria-label="Local changes"
        className="[font-variant-numeric:tabular-nums]"
        actions={
          <>
            {branchPr ? <BranchPrStatusChip pr={branchPr} /> : null}
            {!isolated && totalCount > 0 ? (
              <DropdownMenu>
                <DropdownMenuTrigger
                  render={
                    <Button
                      type="button"
                      size="xs"
                      className="h-6 shrink-0 gap-1 rounded-full bg-ink px-2.5 text-xs font-medium text-bg hover:bg-ink/90"
                    />
                  }
                >
                  Create Branch & Commit
                  <ChevronDown className="size-3 opacity-70" aria-hidden />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" sideOffset={4} className="w-56">
                  <DropdownMenuGroup>
                    <DropdownMenuItem
                      onClick={() => {
                        selectAll()
                        scrollToCommit()
                      }}
                    >
                      Commit selected…
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      onClick={() => {
                        selectAll()
                        scrollToCommit()
                      }}
                    >
                      Create Branch & Commit…
                    </DropdownMenuItem>
                    {hasRemote && prStatus?.ghAvailable ? (
                      <DropdownMenuItem onClick={() => setCreatePrOpen(true)}>
                        Create PR…
                      </DropdownMenuItem>
                    ) : null}
                  </DropdownMenuGroup>
                </DropdownMenuContent>
              </DropdownMenu>
            ) : null}
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label="Refresh changes"
              title="Refresh changes"
              onClick={handleRefresh}
              className={cn("h-6 w-6", panelChromeIconClass)}
            >
              <RefreshCw
                className={cn(
                  "h-3.5 w-3.5",
                  (isFetching || isRepoFetching) && "animate-spin",
                )}
                aria-hidden
              />
            </Button>
          </>
        }
      >
        <div
          className="flex min-w-0 items-center gap-1.5 text-sm tracking-[var(--tracking-caption)]"
          aria-label={branch ? `Local on ${branch}` : "Local changes"}
        >
          <span className="shrink-0 text-ink-muted">Local</span>
          {branch ? (
            <>
              <span className="shrink-0 text-ink-faint" aria-hidden>
                |
              </span>
              <span className="min-w-0 truncate text-ink-muted" title={branch}>
                {branch}
              </span>
            </>
          ) : null}
        </div>
      </PanelToolbar>

      {totalCount > 0 ? (
        <div className="flex h-6 shrink-0 items-center gap-2 border-b border-stroke-3 px-2.5 [font-variant-numeric:tabular-nums]">
          <span className="min-w-0 truncate text-sm font-medium text-ink">
            {totalCount} Uncommitted Change{totalCount === 1 ? "" : "s"}
          </span>
          <DiffStat
            summary={totals}
            size="sm"
            compact={false}
            className="shrink-0"
          />
          <div className="min-w-0 flex-1" />
          {!isolated ? (
            <>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label="Select all files"
                title="Select all"
                onClick={selectAll}
                className="h-5 w-5 text-ink-muted hover:bg-fill-4 hover:text-ink"
              >
                <PlusSquare className="h-3.5 w-3.5" aria-hidden />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label="Discard all changes"
                title="Discard all"
                onClick={() => setConfirmDiscard(true)}
                className="h-5 w-5 text-ink-muted hover:bg-fill-4 hover:text-ink"
              >
                <Undo2 className="h-3.5 w-3.5" aria-hidden />
              </Button>
            </>
          ) : null}
        </div>
      ) : null}

      {statusIsError ? (
        <ToolQueryError
          variant="banner"
          title="Couldn't load changes"
          error={statusError}
          fallbackMessage="Failed to read git status for this session."
          onRetry={() => void refetch()}
          retrying={isFetching}
        />
      ) : null}
      {error ? (
        <ErrorBanner
          message={error}
          className="rounded-none border-x-0 border-t-0 px-2.5 py-1.5 text-xs"
          onDismiss={() => setError(null)}
        />
      ) : null}

      <div
        ref={listRef}
        className={cn(
          "min-h-0 flex-1 overflow-y-auto overscroll-contain",
          "[scrollbar-width:thin] [scrollbar-color:var(--color-stroke-3)_transparent]",
        )}
      >
        {statusIsError && !summary ? (
          <ToolQueryError
            title="Couldn't load changes"
            error={statusError}
            fallbackMessage="Failed to read git status for this session."
            onRetry={() => void refetch()}
            retrying={isFetching}
            className="py-12"
          />
        ) : statusPending && !summary ? (
          <div className="flex items-center justify-center gap-2 px-2.5 py-12 text-sm text-ink-muted">
            <Spinner className="size-3.5" />
            Loading changes…
          </div>
        ) : totalCount === 0 ? (
          <EmptyState
            className="px-2.5"
            icon={<GitMerge className="h-6 w-6" aria-hidden />}
            title="Working tree clean"
            description={branch ? `on ${branch}` : undefined}
          />
        ) : (
          <div
            role="list"
            className="relative w-full py-1"
            style={{ height: listHeight + (truncated ? 28 : 0) }}
          >
            {virtualItems.map((vItem) => {
              const file = files[vItem.index]
              if (!file) return null
              return (
                <div
                  key={vItem.key}
                  role="listitem"
                  data-index={vItem.index}
                  ref={virtualizer.measureElement}
                  className="absolute top-0 left-0 w-full"
                  style={{
                    transform: `translateY(${Math.round(vItem.start)}px)`,
                  }}
                >
                  <FileRow
                    file={file}
                    sessionId={sessionId}
                    isolated={isolated}
                    expanded={expandedPath === file.path}
                    onToggle={toggleExpand}
                    onError={setError}
                    selectable={!isolated}
                    selected={selected.has(file.path)}
                    onToggleSelected={toggleSelected}
                  />
                </div>
              )
            })}
            {truncated ? (
              <p
                className="absolute left-0 w-full px-2.5 py-1.5 text-center text-xs text-ink-faint"
                style={{ top: listHeight }}
              >
                +{totalCount - files.length} more files not shown
              </p>
            ) : null}
          </div>
        )}
      </div>

      {!isolated && sessionId ? (
        <div ref={commitCenterRef}>
          <CommitCenter
            sessionId={sessionId}
            cwd={cwd}
            selectedPaths={selectedPaths}
            totalFiles={files.length}
            onError={setError}
          />
        </div>
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
            disabled={workspace.busy}
            onClick={() => void workspace.integrate()}
          >
            {workspace.busy ? <Spinner data-icon="inline-start" /> : null}
            <GitMerge className="h-3 w-3" aria-hidden /> Keep{aggregateSuffix}
          </Button>
        </div>
      ) : null}

      <ConfirmDialog
        open={confirmDiscard}
        title={`Undo${aggregateSuffix === "" ? "" : " all"} changes?`}
        description={
          isolated
            ? files.length === 1
              ? "This discards the isolated workspace's change and reverts the file to its base state."
              : "This discards the isolated workspace and reverts every changed file to its base state."
            : "This reverts every listed file to its base state."
        }
        confirmLabel={`Undo${aggregateSuffix}`}
        danger
        isLoading={workspace.busy || discardBusy}
        onConfirm={() => {
          void handleDiscardAll()
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
    </div>
  )
}

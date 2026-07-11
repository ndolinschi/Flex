import { useEffect, useMemo, useRef, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { GitMerge, RefreshCw, XCircle } from "lucide-react"
import { Button, IconButton, ScrollArea } from "../../atoms"
import { ConfirmDialog } from "../../molecules"
import { useWorkspaceActions } from "../../../hooks/useWorkspaceActions"
import {
  gitBranch,
  gitIsRepo,
  gitStatusSinceBaseline,
  isIsolated,
} from "../../../lib/tauri"
import type { SessionMeta } from "../../../lib/types"
import { useAppStore } from "../../../stores/appStore"
import { cn, formatTokens } from "../../../lib/utils"
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
  const { data: isRepo = true } = useQuery({
    queryKey: ["git-is-repo", cwd],
    queryFn: () => gitIsRepo(cwd),
    enabled: !!cwd,
    staleTime: 15_000,
  })

  const { data: files = [], refetch, isFetching } = useQuery({
    queryKey: ["git-status", cwd, sessionId],
    queryFn: () => gitStatusSinceBaseline(sessionId!),
    enabled: !!cwd && !!sessionId && isRepo,
    refetchInterval: 5_000,
    refetchOnWindowFocus: true,
  })

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
      void queryClient.invalidateQueries({ queryKey: ["git-status"] })
    }
    prevStreaming.current = isStreaming
  }, [isStreaming, queryClient])

  const totals = useMemo(
    () =>
      files.reduce(
        (acc, f) => ({
          added: acc.added + (f.added ?? 0),
          removed: acc.removed + (f.removed ?? 0),
        }),
        { added: 0, removed: 0 },
      ),
    [files],
  )

  // design: the aggregate bar's buttons read "Keep All"/"Undo All" when
  // multiple files are pending, singular "Keep"/"Undo" for exactly one — one
  // bar, label swaps, not separate components.
  const aggregateSuffix = files.length === 1 ? "" : " All"

  if (!active) {
    return (
      <div className="flex flex-1 items-center justify-center px-6 text-center">
        <p className="text-sm text-ink-muted">No active session.</p>
      </div>
    )
  }

  // No git repo in this cwd at all — calm empty state, not an error. A repo
  // with an unborn HEAD (no commits yet) is still `isRepo === true` and
  // keeps the regular UI below (see `git_is_repo`'s doc comment).
  if (!isRepo) {
    return (
      <div className="flex flex-1 items-center justify-center px-6 text-center">
        <p className="text-sm text-ink-muted">Not a git repository.</p>
      </div>
    )
  }

  return (
    <>
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-stroke-3 px-3 text-sm [font-variant-numeric:tabular-nums]">
        <span className="min-w-0 truncate text-ink-secondary">
          {files.length === 0
            ? "No changes"
            : `${files.length} file${files.length === 1 ? "" : "s"} changed`}
        </span>
        <span className="ml-auto flex shrink-0 items-center gap-1.5">
          {totals.added > 0 ? (
            <span className="text-green">+{formatTokens(totals.added)}</span>
          ) : null}
          {totals.removed > 0 ? (
            <span className="text-red">-{formatTokens(totals.removed)}</span>
          ) : null}
          <IconButton
            label="Refresh changes"
            onClick={() => void refetch()}
            className="h-6 w-6"
          >
            <RefreshCw
              className={cn("h-3 w-3", isFetching && "animate-spin")}
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

      <ScrollArea className="min-h-0 flex-1">
        {files.length === 0 ? (
          <p className="px-6 py-8 text-center text-sm text-ink-muted">
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
              />
            ))}
          </ul>
        )}
      </ScrollArea>

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

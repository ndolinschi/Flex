import { useEffect, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { ExternalLink, FileCode2, GitPullRequest, Loader2 } from "lucide-react"
import type { SessionMeta } from "../../../lib/types"
import { gitPrDiff, gitPrFiles, gitPrStatus } from "../../../lib/tauri"
import { openExternalUrl } from "../../../lib/openExternalUrl"
import { basename, cn } from "../../../lib/utils"
import { Spinner } from "../../atoms"
import { Button } from "@/components/ui/button"
import {
  DiffView,
  EmptyState,
  PanelSideRail,
  PanelToolbar,
  PanelToolbarTitle,
  ToolQueryError,
} from "../../molecules"
import { ScrollArea } from "@/components/ui/scroll-area"

type PrTabProps = {
  active: SessionMeta | undefined
}

export const PrTab = ({ active }: PrTabProps) => {
  const cwd = active?.cwd
  const [selectedPath, setSelectedPath] = useState<string | null>(null)

  const prQuery = useQuery({
    queryKey: ["git-pr-status", cwd ?? ""],
    queryFn: () => gitPrStatus(cwd!),
    enabled: !!cwd,
    staleTime: 60_000,
    refetchInterval: 30_000,
  })

  const filesQuery = useQuery({
    queryKey: ["git-pr-files", cwd ?? "", prQuery.data?.pr?.number ?? 0],
    queryFn: () => gitPrFiles(cwd!),
    enabled: !!cwd && !!prQuery.data?.pr,
    staleTime: 30_000,
    refetchInterval: 60_000,
  })

  const files = filesQuery.data ?? []

  useEffect(() => {
    if (files.length === 0) {
      setSelectedPath(null)
      return
    }
    setSelectedPath((prev) =>
      prev && files.includes(prev) ? prev : (files[0] ?? null),
    )
  }, [files])

  const diffQuery = useQuery({
    queryKey: [
      "git-pr-diff",
      cwd ?? "",
      prQuery.data?.pr?.number ?? 0,
      selectedPath ?? "",
    ],
    queryFn: () => gitPrDiff(cwd!, selectedPath ?? undefined),
    enabled: !!cwd && !!prQuery.data?.pr && !!selectedPath,
    staleTime: 30_000,
  })

  const pr = prQuery.data?.pr
  const failing = pr?.checksSummary.includes("failing")
  const pending = pr?.checksSummary.includes("pending")

  if (!cwd) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <EmptyState
          className="min-h-0 flex-1"
          icon={<GitPullRequest className="h-6 w-6" aria-hidden />}
          title="No project"
          description="Open a session with a project to review a pull request."
        />
      </div>
    )
  }

  if (prQuery.isLoading) {
    return (
      <div className="flex h-full min-h-0 flex-col items-center justify-center gap-2 text-sm text-ink-muted">
        <Spinner size="sm" /> Looking up pull request…
      </div>
    )
  }

  if (prQuery.isError) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <ToolQueryError
          title="Couldn't load pull request"
          error={prQuery.error}
          fallbackMessage="Failed to look up the pull request for this branch."
          onRetry={() => void prQuery.refetch()}
          retrying={prQuery.isFetching}
        />
      </div>
    )
  }

  if (!pr) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <EmptyState
          className="min-h-0 flex-1"
          icon={<GitPullRequest className="h-6 w-6" aria-hidden />}
          title="No pull request"
          description="Create a PR for this branch and it will show up here for review."
        />
      </div>
    )
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PanelToolbar
        aria-label="Pull request"
        actions={
          <>
            <span
              className={cn(
                "shrink-0 text-xs uppercase tracking-[var(--tracking-caption)]",
                failing
                  ? "text-destructive"
                  : pending
                    ? "text-ink-muted"
                    : "text-success",
              )}
              title={`${pr.state} · ${pr.checksSummary}`}
            >
              {pr.checksSummary}
            </span>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-6 shrink-0 gap-1 px-2 text-xs"
              onClick={() => void openExternalUrl(pr.url)}
              aria-label={`Open pull request #${pr.number}`}
            >
              <ExternalLink className="h-3 w-3" aria-hidden />
              Open
            </Button>
          </>
        }
      >
        <PanelToolbarTitle icon={<GitPullRequest aria-hidden />}>
          <span className="font-medium">#{pr.number}</span>{" "}
          <span className="text-ink-secondary">{pr.title}</span>
        </PanelToolbarTitle>
      </PanelToolbar>

      {filesQuery.isError ? (
        <ToolQueryError
          title="Couldn't list PR files"
          error={filesQuery.error}
          fallbackMessage="Failed to list files in this pull request."
          onRetry={() => void filesQuery.refetch()}
          retrying={filesQuery.isFetching}
        />
      ) : filesQuery.isLoading ? (
        <div className="flex min-h-0 flex-1 items-center justify-center gap-2 text-sm text-ink-muted">
          <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
          Loading files…
        </div>
      ) : files.length === 0 ? (
        <EmptyState
          className="min-h-0 flex-1"
          title="No files in PR"
          description="This pull request has no file changes, or gh could not list them."
        />
      ) : (
        <div className="flex min-h-0 flex-1">
          <PanelSideRail
            width={180}
            header={
              <span className="tabular-nums">
                {files.length} file{files.length === 1 ? "" : "s"}
              </span>
            }
          >
            <ScrollArea className="min-h-0 flex-1 py-1.5">
              <ul>
                {files.map((path) => {
                  const activeFile = path === selectedPath
                  return (
                    <li key={path}>
                      <Button
                        type="button"
                        variant="ghost"
                        onClick={() => setSelectedPath(path)}
                        title={path}
                        className={cn(
                          "h-auto w-full justify-start gap-1.5 px-2.5 py-1.5 text-xs font-normal",
                          activeFile
                            ? "bg-fill-2 text-ink hover:bg-fill-2"
                            : "text-ink-secondary hover:bg-fill-4 hover:text-ink",
                        )}
                      >
                        <FileCode2
                          className="h-3 w-3 shrink-0 text-icon-3"
                          aria-hidden
                        />
                        <span className="min-w-0 truncate font-mono">
                          {basename(path)}
                        </span>
                      </Button>
                    </li>
                  )
                })}
              </ul>
            </ScrollArea>
          </PanelSideRail>

          <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            {selectedPath ? (
              <div className="flex h-6 shrink-0 items-center border-b border-stroke-3 px-2.5 text-xs text-ink-muted">
                <span className="min-w-0 truncate font-mono" title={selectedPath}>
                  {selectedPath}
                </span>
              </div>
            ) : null}
            {diffQuery.isLoading ? (
              <div className="flex min-h-0 flex-1 items-center justify-center gap-2 text-sm text-ink-muted">
                <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
                Loading diff…
              </div>
            ) : diffQuery.isError ? (
              <ToolQueryError
                title="Couldn't load file diff"
                error={diffQuery.error}
                fallbackMessage="Failed to load this file's pull request diff."
                onRetry={() => void diffQuery.refetch()}
                retrying={diffQuery.isFetching}
              />
            ) : (
              <ScrollArea className="min-h-0 flex-1">
                {diffQuery.data && diffQuery.data.trim().length > 0 ? (
                  <DiffView diff={diffQuery.data} />
                ) : (
                  <EmptyState
                    className="py-12"
                    title="No diff"
                    description="No diff for this file in the pull request."
                  />
                )}
              </ScrollArea>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

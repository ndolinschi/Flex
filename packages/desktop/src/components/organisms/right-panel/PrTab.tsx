import { useQuery } from "@tanstack/react-query"
import { ExternalLink, GitPullRequest, Loader2 } from "@/components/icons"
import type { SessionMeta } from "../../../lib/types"
import { gitPrDiff, gitPrStatus } from "../../../lib/tauri"
import { openExternalUrl } from "../../../lib/openExternalUrl"
import { cn } from "../../../lib/utils"
import { Button, ScrollArea, Spinner } from "../../atoms"
import { DiffView, EmptyState } from "../../molecules"

type PrTabProps = {
  active: SessionMeta | undefined
}

/** Right-panel PR review — only meaningful when `gitPrStatus` finds a PR for
 * the session cwd's current branch. Header metadata + full `gh pr diff`. */
export const PrTab = ({ active }: PrTabProps) => {
  const cwd = active?.cwd

  const prQuery = useQuery({
    queryKey: ["git-pr-status", cwd ?? ""],
    queryFn: () => gitPrStatus(cwd!),
    enabled: !!cwd,
    refetchInterval: 30_000,
  })

  const diffQuery = useQuery({
    queryKey: ["git-pr-diff", cwd ?? "", prQuery.data?.pr?.number ?? 0],
    queryFn: () => gitPrDiff(cwd!),
    enabled: !!cwd && !!prQuery.data?.pr,
    refetchInterval: 60_000,
  })

  const pr = prQuery.data?.pr
  const failing = pr?.checksSummary.includes("failing")
  const pending = pr?.checksSummary.includes("pending")

  if (!cwd) {
    return (
      <EmptyState
        icon={<GitPullRequest className="h-7 w-7" aria-hidden />}
        title="No project"
        description="Open a session with a project to review a pull request."
      />
    )
  }

  if (prQuery.isLoading) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-sm text-ink-muted">
        <Spinner size="sm" /> Looking up pull request…
      </div>
    )
  }

  if (!pr) {
    return (
      <EmptyState
        icon={<GitPullRequest className="h-7 w-7" aria-hidden />}
        title="No pull request"
        description="Create a PR for this branch and it will show up here for review."
      />
    )
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 flex-col gap-1.5 border-b border-stroke-3 px-2.5 py-2">
        <div className="flex items-start gap-2">
          <GitPullRequest
            className="mt-0.5 h-3.5 w-3.5 shrink-0 text-icon-3"
            aria-hidden
          />
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm text-ink">
              <span className="font-medium">#{pr.number}</span>{" "}
              <span className="text-ink-secondary">{pr.title}</span>
            </p>
            <p className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-ink-muted">
              <span className="uppercase tracking-[var(--tracking-caption)]">
                {pr.state}
              </span>
              <span
                className={cn(
                  failing
                    ? "text-danger"
                    : pending
                      ? "text-ink-muted"
                      : "text-success",
                )}
              >
                {pr.checksSummary}
              </span>
            </p>
          </div>
          <Button
            variant="ghost"
            size="sm"
            className="shrink-0"
            onClick={() => void openExternalUrl(pr.url)}
          >
            <ExternalLink className="h-3 w-3" aria-hidden />
            Open
          </Button>
        </div>
      </div>

      <ScrollArea className="min-h-0 flex-1">
        {diffQuery.isLoading ? (
          <div className="flex items-center justify-center gap-2 px-3 py-8 text-sm text-ink-muted">
            <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
            Loading diff…
          </div>
        ) : diffQuery.data && diffQuery.data.trim().length > 0 ? (
          <DiffView diff={diffQuery.data} />
        ) : (
          <p className="px-3 py-6 text-center text-sm text-ink-muted">
            No diff for this pull request.
          </p>
        )}
      </ScrollArea>
    </div>
  )
}

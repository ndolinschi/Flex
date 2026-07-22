import { useQuery } from "@tanstack/react-query"
import { ExternalLink, GitPullRequest, Loader2 } from "lucide-react"
import type { SessionMeta } from "../../../lib/types"
import { gitPrDiff, gitPrStatus } from "../../../lib/tauri"
import { openExternalUrl } from "../../../lib/openExternalUrl"
import { cn } from "../../../lib/utils"
import { Spinner } from "../../atoms"
import { Button } from "@/components/ui/button"
import { DiffView, EmptyState } from "../../molecules"
import { ScrollArea } from "@/components/ui/scroll-area"

type PrTabProps = {
  active: SessionMeta | undefined
}

/** Right-panel PR review — only meaningful when `gitPrStatus` finds a PR for
 * the session cwd's current branch. Header metadata + full `gh pr diff`.
 * Chrome matches Browser/Changes: 30px `--header-height` row + `px-2.5`. */
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
        icon={<GitPullRequest className="h-6 w-6" aria-hidden />}
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
        icon={<GitPullRequest className="h-6 w-6" aria-hidden />}
        title="No pull request"
        description="Create a PR for this branch and it will show up here for review."
      />
    )
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Primary chrome — fixed 30px row (Browser recipe). */}
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1.5 px-2.5">
        <GitPullRequest
          className="h-3.5 w-3.5 shrink-0 text-ink-faint"
          aria-hidden
        />
        <span className="min-w-0 flex-1 truncate text-sm text-ink">
          <span className="font-medium">#{pr.number}</span>{" "}
          <span className="text-ink-secondary">{pr.title}</span>
        </span>
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
      </div>

      <ScrollArea className="min-h-0 flex-1">
        {diffQuery.isLoading ? (
          <div className="flex items-center justify-center gap-2 px-2.5 py-8 text-sm text-ink-muted">
            <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
            Loading diff…
          </div>
        ) : diffQuery.data && diffQuery.data.trim().length > 0 ? (
          <DiffView diff={diffQuery.data} />
        ) : (
          <EmptyState
            title="No diff"
            description="No diff for this pull request."
          />
        )}
      </ScrollArea>
    </div>
  )
}

import { useQuery } from "@tanstack/react-query"
import { ArrowRight } from "lucide-react"
import { gitIsRepo, gitStatusSinceBaseline } from "../../lib/tauri"
import { useAppStore } from "../../stores/appStore"
import { DiffStat } from "../atoms"

type FilesChangedCardProps = {
  cwd?: string
  sessionId?: string | null
}

/**
 * Compact "N files changed" summary, rendered in the chat feed after the
 * latest turn once the session stops streaming and the working tree has
 * pending changes. Shares the `["git-status", cwd, sessionId]` query with
 * the right panel's Changes tab (same key → same cache entry, no extra
 * polling) — but deliberately does NOT repeat the per-file list here; that's
 * the Changes tab's job. This card is just the end-of-turn headline + a
 * "Review" affordance that opens/focuses that tab, so the full list only
 * ever renders once on screen.
 */
export const FilesChangedCard = ({ cwd, sessionId }: FilesChangedCardProps) => {
  const setRightPanelOpen = useAppStore((s) => s.setRightPanelOpen)
  const setRightPanelTab = useAppStore((s) => s.setRightPanelTab)

  // Gate on the cwd being a git repo — see ContextBar/RightPanel's `isRepo`
  // gating for the full rationale; this card must return null (not an
  // error) for a non-git session's cwd.
  const { data: isRepo } = useQuery({
    queryKey: ["git-is-repo", cwd ?? ""],
    queryFn: () => gitIsRepo(cwd!),
    enabled: !!cwd,
    staleTime: 0,
    refetchOnMount: "always",
    refetchOnWindowFocus: true,
    refetchInterval: 5_000,
  })

  const { data: summary } = useQuery({
    queryKey: ["git-status", cwd ?? "", sessionId ?? null],
    queryFn: () => gitStatusSinceBaseline(sessionId!),
    enabled: !!cwd && !!sessionId && isRepo !== false,
    staleTime: 30_000,
    // Card mounts when streaming ends — always re-check so a mid-turn
    // cached empty result doesn't hide real post-turn changes.
    refetchOnMount: "always",
  })

  // `totalCount`/totals come from the summary so the count and +/- badge
  // always reflect every changed file, even past the server-side row cap —
  // this card never touches `summary.files` at all since it doesn't list them.
  const totalCount = summary?.totalCount ?? 0

  if (!isRepo || totalCount === 0) return null

  // Same `["git-status", cwd, sessionId]` query as ContextBar's CommitBar
  // pill and the Changes tab, so all totals always agree.
  const totals = {
    added: summary?.totalAdded ?? 0,
    removed: summary?.totalRemoved ?? 0,
  }

  const handleReview = () => {
    setRightPanelOpen(true)
    setRightPanelTab("changes")
  }

  return (
    <div className="flex h-9 items-center justify-between rounded-lg border border-stroke-3 bg-transparent px-3">
      <span className="flex min-w-0 items-center gap-1.5">
        <span className="truncate text-[13px] text-ink">
          {totalCount} file{totalCount === 1 ? "" : "s"} changed
        </span>
        <DiffStat summary={totals} size="sm" />
      </span>
      <button
        type="button"
        onClick={handleReview}
        className="flex shrink-0 items-center gap-1 text-xs text-accent transition-opacity hover:opacity-80"
      >
        Review
        <ArrowRight className="h-3 w-3" aria-hidden />
      </button>
    </div>
  )
}

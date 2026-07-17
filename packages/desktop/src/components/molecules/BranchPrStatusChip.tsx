import { GitPullRequest } from "@/components/icons"
import type { BranchPrInfo } from "../../lib/tauri"
import { openExternalUrl } from "../../lib/openExternalUrl"
import { cn } from "../../lib/utils"
import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from "@/components/ui/hover-card"

type BranchPrStatusChipProps = {
  pr: BranchPrInfo
  className?: string
}

/** Compact current-branch PR chip for the Changes header — number, title,
 * and CI summary. Opens the PR in the system browser on click. */
export const BranchPrStatusChip = ({ pr, className }: BranchPrStatusChipProps) => {
  const failing = pr.checksSummary.includes("failing")
  const pending = pr.checksSummary.includes("pending")

  return (
    <HoverCard openDelay={200} closeDelay={100}>
      <HoverCardTrigger asChild>
        <button
          type="button"
          onClick={() => void openExternalUrl(pr.url)}
          aria-label={`Open pull request #${pr.number}`}
          className={cn(
            "flex max-w-[min(100%,18rem)] items-center gap-1.5 rounded-md px-1.5 py-0.5",
            "text-xs tracking-[var(--tracking-caption)] text-ink-secondary",
            "transition-colors duration-[var(--duration-fast)]",
            "hover:bg-fill-3 hover:text-ink",
            className,
          )}
        >
          <GitPullRequest className="h-3 w-3 shrink-0 text-icon-3" aria-hidden />
          <span className="shrink-0 font-medium text-ink">#{pr.number}</span>
          <span className="min-w-0 truncate">{pr.title}</span>
          <span
            className={cn(
              "shrink-0",
              failing
                ? "text-danger"
                : pending
                  ? "text-ink-muted"
                  : "text-success",
            )}
          >
            {pr.checksSummary}
          </span>
        </button>
      </HoverCardTrigger>
      <HoverCardContent
        side="bottom"
        align="start"
        className="w-64 gap-1 border-stroke-3 bg-panel p-2.5 text-xs shadow-[var(--shadow-md)]"
      >
        <p className="font-medium text-ink">
          #{pr.number} · {pr.title}
        </p>
        <p
          className={cn(
            failing ? "text-danger" : pending ? "text-ink-muted" : "text-success",
          )}
        >
          Checks: {pr.checksSummary}
        </p>
        <p className="text-ink-faint">Click to open in browser</p>
      </HoverCardContent>
    </HoverCard>
  )
}

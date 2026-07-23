import { GitPullRequest } from "lucide-react"
import type { BranchPrInfo } from "../../lib/tauri"
import { openExternalUrl } from "../../lib/openExternalUrl"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"

type BranchPrStatusChipProps = {
  pr: BranchPrInfo
  className?: string
}

export const BranchPrStatusChip = ({ pr, className }: BranchPrStatusChipProps) => {
  const failing = pr.checksSummary.includes("failing")
  const pending = pr.checksSummary.includes("pending")

  return (
    <Button
      variant="ghost"
      onClick={() => void openExternalUrl(pr.url)}
      title={`${pr.title} — ${pr.checksSummary}`}
      aria-label={`Open pull request #${pr.number}`}
      className={cn(
        "h-auto max-w-[min(100%,18rem)] gap-1.5 px-1.5 py-0.5 font-normal",
        "text-xs tracking-[var(--tracking-caption)] text-ink-secondary",
        "hover:bg-fill-4 hover:text-ink",
        className,
      )}
    >
      <GitPullRequest className="h-3 w-3 shrink-0 text-icon-3" aria-hidden />
      <span className="shrink-0 font-medium text-ink">#{pr.number}</span>
      <span className="min-w-0 truncate">{pr.title}</span>
      <span
        className={cn(
          "shrink-0",
          failing ? "text-destructive" : pending ? "text-ink-muted" : "text-success",
        )}
      >
        {pr.checksSummary}
      </span>
    </Button>
  )
}

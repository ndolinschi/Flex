import { useState } from "react"
import { ChevronRight } from "@/components/icons"
import { cn, formatTokens } from "../../lib/utils"
import { Collapsible } from "./Collapsible"
import { MarkdownBody } from "./MarkdownBody"
import { Marker, MarkerContent } from "@/components/ui/marker"

type CompactionCardProps = {
  summaryMarkdown: string
  strategy: string
  tokensBefore?: number
  tokensAfter?: number
  /** Fired when expansion changes so the timeline can re-stick to bottom. */
  onLayoutChange?: () => void
}

const isAutoStrategy = (strategy: string): boolean =>
  strategy.startsWith("auto_")

/** Settled context-compaction boundary: hairline divider, readable title,
 * optional token delta, expandable summary of what the model will see. */
export const CompactionCard = ({
  summaryMarkdown,
  strategy,
  tokensBefore,
  tokensAfter,
  onLayoutChange,
}: CompactionCardProps) => {
  const [expanded, setExpanded] = useState(false)
  const hasSummary = summaryMarkdown.trim().length > 0
  const title = isAutoStrategy(strategy)
    ? "Context compacted to free space"
    : "Context compacted"
  const sizes =
    typeof tokensBefore === "number" && typeof tokensAfter === "number"
      ? `${formatTokens(tokensBefore)} → ${formatTokens(tokensAfter)} tokens`
      : null

  const handleToggle = () => {
    if (!hasSummary) return
    setExpanded((v) => !v)
    onLayoutChange?.()
  }

  return (
    <div className="animate-row-fade flex flex-col gap-1.5 py-1">
      <Marker
        variant="border"
        asChild
        className="border-stroke-3 text-ink-muted"
      >
        <button
          type="button"
          onClick={handleToggle}
          aria-expanded={hasSummary ? expanded : undefined}
          disabled={!hasSummary}
          className={cn(
            "group flex w-full items-center gap-1.5 text-left",
            hasSummary && "cursor-pointer hover:text-ink",
          )}
        >
          {hasSummary ? (
            <ChevronRight
              className={cn(
                "h-2.5 w-2.5 shrink-0 text-icon-3 transition-transform duration-[var(--duration-fast)]",
                expanded && "rotate-90",
              )}
              aria-hidden
            />
          ) : null}
          <MarkerContent className="min-w-0 font-medium text-ink-secondary">
            {title}
          </MarkerContent>
          {sizes ? (
            <span className="shrink-0 text-ink-faint [font-variant-numeric:tabular-nums]">
              · {sizes}
            </span>
          ) : null}
        </button>
      </Marker>
      {hasSummary ? (
        <Collapsible open={expanded}>
          <div className="pl-4 text-ink-muted">
            <MarkdownBody content={summaryMarkdown} />
          </div>
        </Collapsible>
      ) : null}
    </div>
  )
}

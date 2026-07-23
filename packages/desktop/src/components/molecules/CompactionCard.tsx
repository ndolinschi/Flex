import { useState } from "react"
import { ChevronRight } from "lucide-react"
import { cn, formatTokens } from "../../lib/utils"
import { Collapsible } from "./Collapsible"
import { MarkdownBody } from "./MarkdownBody"
import { Button } from "@/components/ui/button"
import { Marker, MarkerIcon, MarkerContent } from "@/components/ui/marker"

type CompactionCardProps = {
  summaryMarkdown: string
  strategy: string
  tokensBefore?: number
  tokensAfter?: number
  onLayoutChange?: () => void
}

const isAutoStrategy = (strategy: string): boolean =>
  strategy.startsWith("auto_")

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
        variant="separator"
        render={
          hasSummary ? (
            <Button
              type="button"
              variant="ghost"
              onClick={handleToggle}
              aria-expanded={expanded}
              className="h-auto w-full justify-start rounded-none border-0 bg-transparent p-0 font-normal hover:bg-transparent"
            />
          ) : undefined
        }
        className={cn(
          "text-sm text-ink-muted",
          hasSummary && "cursor-pointer hover:text-ink",
        )}
      >
        {hasSummary ? (
          <MarkerIcon>
            <ChevronRight
              className={cn(
                "h-2.5 w-2.5 transition-transform duration-[var(--duration-fast)]",
                expanded && "rotate-90",
              )}
              aria-hidden
            />
          </MarkerIcon>
        ) : null}
        <MarkerContent>
          <span className="font-medium text-ink-secondary">{title}</span>
          {sizes ? (
            <span className="ml-1 shrink-0 text-ink-faint [font-variant-numeric:tabular-nums]">
              · {sizes}
            </span>
          ) : null}
        </MarkerContent>
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

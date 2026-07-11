import { useEffect, useRef, useState, type ReactNode } from "react"
import { ChevronRight } from "lucide-react"
import { RunningDot } from "../atoms"
import { Collapsible } from "./Collapsible"
import type { VerificationVerdict } from "../../lib/types"
import { cn, formatCost, formatDuration, formatTokens } from "../../lib/utils"

const VERDICT_GLYPH: Record<VerificationVerdict["outcome"], string> = {
  pass: "✓",
  fail: "✗",
  inconclusive: "?",
}

const verdictClasses = (outcome: VerificationVerdict["outcome"]): string => {
  if (outcome === "pass") return "text-green"
  if (outcome === "fail") return "text-red"
  return "text-yellow"
}

type WorkGroupProps = {
  /** True while the turn is still streaming — keeps the group expanded. */
  isOpen: boolean
  durationMs?: number
  /** Turn cost (USD) shown next to the duration when the group is collapsed. */
  costUsd?: number
  /** Total tokens for the turn, shown next to cost. */
  totalTokens?: number
  /** Latest `Verify` call's verdict among this group's rows, if any — shown
   * as a small glyph next to the duration so it's visible even collapsed. */
  verdict?: VerificationVerdict
  children: ReactNode
  /** Fired when expansion changes so the timeline can re-stick to bottom. */
  onLayoutChange?: () => void
  /** Outer margin — set by the timeline's content-driven feed rhythm. */
  className?: string
}

/** "Worked for Xs" collapsible wrapper around a turn's work rows. */
export const WorkGroup = ({
  isOpen,
  durationMs,
  costUsd,
  totalTokens,
  verdict,
  children,
  onLayoutChange,
  className,
}: WorkGroupProps) => {
  const [expanded, setExpanded] = useState(isOpen)
  const prevOpen = useRef(isOpen)

  useEffect(() => {
    if (prevOpen.current !== isOpen) {
      // Live turn opens the group; completion collapses it.
      setExpanded(isOpen)
      prevOpen.current = isOpen
      onLayoutChange?.()
    }
  }, [isOpen, onLayoutChange])

  const handleToggle = () => {
    if (isOpen) return
    setExpanded((v) => !v)
    onLayoutChange?.()
  }

  return (
    <div className={cn("flex flex-col", className)}>
      <button
        type="button"
        onClick={handleToggle}
        aria-expanded={expanded}
        className={cn(
          "group flex min-h-6 w-full items-center gap-1.5 text-left text-base",
          !isOpen && "cursor-pointer animate-end-turn-in",
        )}
      >
        {isOpen ? (
          <>
            <RunningDot className="-ml-1 h-4 w-4" />
            <span className="animate-shimmer-text">Working</span>
          </>
        ) : (
          <>
            <span className="text-ink-secondary [font-variant-numeric:tabular-nums]">
              {typeof durationMs === "number"
                ? `Worked for ${formatDuration(durationMs)}`
                : "Worked"}
            </span>
            {typeof totalTokens === "number" && totalTokens > 0 ? (
              <span className="text-ink-faint [font-variant-numeric:tabular-nums]">
                · {formatTokens(totalTokens)} tokens
              </span>
            ) : null}
            {typeof costUsd === "number" && costUsd > 0 ? (
              <span className="text-ink-faint [font-variant-numeric:tabular-nums]">
                · {formatCost(costUsd)}
              </span>
            ) : null}
            {verdict ? (
              <span
                className={cn("shrink-0", verdictClasses(verdict.outcome))}
                title={verdict.findings.join(" ") || undefined}
                aria-hidden
              >
                · {VERDICT_GLYPH[verdict.outcome]}
              </span>
            ) : null}
            <ChevronRight
              className={cn(
                "h-2.5 w-2.5 text-icon-3 opacity-0 transition-[transform,opacity] duration-[var(--duration-fast)]",
                "group-hover:opacity-100 group-focus-visible:opacity-100",
                expanded && "rotate-90 opacity-100",
              )}
              aria-hidden
            />
          </>
        )}
      </button>

      <Collapsible open={expanded}>
        <div className="flex flex-col gap-0.5">{children}</div>
      </Collapsible>
    </div>
  )
}

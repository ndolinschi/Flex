import { memo, useEffect, useRef, useState, type ReactNode } from "react"
import { ChevronRight } from "lucide-react"
import { RunningDot } from "../atoms"
import { Collapsible } from "./Collapsible"
import type { VerificationVerdict } from "../../lib/types"
import { cn, formatCost, formatDuration, formatTokens } from "../../lib/utils"
import { Button } from "@/components/ui/button"

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
  isOpen: boolean
  isStreaming?: boolean
  liveStatus?: "working" | "thinking" | "compacting" | "indexing"
  liveNote?: string | null
  durationMs?: number
  costUsd?: number
  totalTokens?: number
  verdict?: VerificationVerdict
  resumeLine?: string | null
  stopped?: boolean
  children: ReactNode
  onLayoutChange?: () => void
  className?: string
}

export const WorkGroup = memo(({
  isOpen,
  isStreaming: _isStreaming = false,
  liveStatus = "working",
  liveNote = null,
  durationMs,
  costUsd,
  totalTokens,
  verdict,
  resumeLine,
  stopped = false,
  children,
  onLayoutChange,
  className,
}: WorkGroupProps) => {
  const [expanded, setExpanded] = useState(isOpen)
  const prevOpen = useRef(isOpen)

  useEffect(() => {
    if (prevOpen.current !== isOpen) {
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

  const durationLabel =
    typeof durationMs === "number" ? formatDuration(durationMs) : null

  const collapsedPrimary = (() => {
    if (stopped) {
      return durationLabel ? `Stopped · ${durationLabel}` : "Stopped"
    }
    if (resumeLine) {
      return durationLabel ? `${resumeLine} · ${durationLabel}` : resumeLine
    }
    if (durationLabel) return `Worked for ${durationLabel}`
    return "Worked"
  })()

  const openLabel =
    liveStatus === "compacting"
      ? "Compacting context…"
      : liveStatus === "indexing"
        ? liveNote?.trim() || "Indexing repository…"
        : liveStatus === "thinking"
          ? "Thinking"
          : "Working"

  return (
    <div className={cn("flex flex-col", className)}>
      {isOpen ? (
        <div className="flex min-h-[var(--end-of-turn-reserved-height)] items-center gap-1 text-base leading-[1.5]">
          <RunningDot className="-ml-1 h-4 w-4" />
          <span className="animate-shimmer-text">{openLabel}</span>
        </div>
      ) : (
        <Button
          variant="ghost"
          onClick={handleToggle}
          aria-expanded={expanded}
          className={cn(
            "group h-auto w-full justify-start gap-1 px-0 py-0 font-normal text-base leading-[1.5]",
            "min-h-[var(--end-of-turn-reserved-height)]",
            "hover:bg-transparent aria-expanded:bg-transparent",
            "cursor-pointer animate-end-turn-in",
          )}
        >
          <span className="min-w-0 truncate text-ink-muted [font-variant-numeric:tabular-nums]">
            {collapsedPrimary}
          </span>
          {typeof totalTokens === "number" && totalTokens > 0 ? (
            <span className="shrink-0 text-ink-faint [font-variant-numeric:tabular-nums]">
              · {formatTokens(totalTokens)} tokens
            </span>
          ) : null}
          {typeof costUsd === "number" && costUsd > 0 ? (
            <span className="shrink-0 text-ink-faint [font-variant-numeric:tabular-nums]">
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
              "h-2.5 w-2.5 shrink-0 text-icon-3 opacity-0 transition-[transform,opacity] duration-[var(--duration-fast)]",
              "group-hover:opacity-100 group-focus-visible:opacity-100",
              expanded && "rotate-90 opacity-100",
            )}
            aria-hidden
          />
        </Button>
      )}

      <Collapsible open={expanded}>
        <div className="flex flex-col gap-0.5">{children}</div>
      </Collapsible>
    </div>
  )
})

WorkGroup.displayName = "WorkGroup"

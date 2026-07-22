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
  /** True while the turn is still streaming — keeps the group expanded and
   * shows the header's RunningDot + shimmering live-status label (the group is
   * only ever open while the turn is live, so `isOpen` alone gates that
   * indicator — see the header render below). */
  isOpen: boolean
  /** Accepted for caller compatibility; no longer used to gate any rendering
   * here. The body used to render a second "Working" row as its last child
   * whenever `isOpen && isStreaming`, duplicating the header's own cue —
   * removed so there's exactly ONE animated status indicator per group. */
  isStreaming?: boolean
  /** Live header label while `isOpen`. Prefer `"thinking"` when the group's
   * rows include streaming thinking so the header owns that cue (ThinkingBlock
   * inside then suppresses its own shimmer via `suppressStatusLabel`). Prefer
   * `"compacting"` while a live `compaction_started` is in flight — the
   * summarizer emits no text, so "Working" would look hung. */
  liveStatus?: "working" | "thinking" | "compacting" | "indexing"
  durationMs?: number
  /** Turn cost (USD) shown next to the duration when the group is collapsed. */
  costUsd?: number
  /** Total tokens for the turn, shown next to cost. */
  totalTokens?: number
  /** Latest `Verify` call's verdict among this group's rows, if any — shown
   * as a small glyph next to the duration so it's visible even collapsed. */
  verdict?: VerificationVerdict
  /** Reference-style resume when collapsed, e.g. "Edited 3 files · Explored 2
 * files · Ran 1 command". Falls back to "Worked for Xs" when absent. */
  resumeLine?: string | null
  /** True when the turn ended via Stop / cancel — collapsed header reads
   * "Stopped" instead of the usual resume / "Worked for" line. */
  stopped?: boolean
  children: ReactNode
  /** Fired when expansion changes so the timeline can re-stick to bottom. */
  onLayoutChange?: () => void
  /** Outer margin — set by the timeline's content-driven feed rhythm. */
  className?: string
}

/** Collapsible wrapper around a turn's work rows — one live status while open
 * ("Compacting context…" / "Thinking" / "Working"), resume line + duration
 * when settled. */
export const WorkGroup = memo(({
  isOpen,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars -- accepted
  // for caller compatibility; see the prop's JSDoc above.
  isStreaming: _isStreaming = false,
  liveStatus = "working",
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
        ? "Indexing repository…"
        : liveStatus === "thinking"
          ? "Thinking"
          : "Working"

  return (
    <div className={cn("flex flex-col", className)}>
      {isOpen ? (
        // Live status is not interactive (toggle is no-op while open) — use a
        // plain row so ghost Button's `aria-expanded:bg-fill-4` does not paint
        // a full-width pill behind "Working" / "Thinking".
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
            // Tool-line density: gap 4, lh 1.5 (Cursor ui-tool-call-line).
            "group h-auto w-full justify-start gap-1 px-0 py-0 font-normal text-base leading-[1.5]",
            "min-h-[var(--end-of-turn-reserved-height)]",
            "hover:bg-transparent aria-expanded:bg-transparent",
            "cursor-pointer animate-end-turn-in",
          )}
        >
          <span className="min-w-0 truncate text-ink-secondary [font-variant-numeric:tabular-nums]">
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

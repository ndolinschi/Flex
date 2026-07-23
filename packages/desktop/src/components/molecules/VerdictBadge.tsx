import { useState } from "react"
import { ChevronRight, LoaderCircle } from "lucide-react"
import type { VerificationVerdict } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Collapsible } from "./Collapsible"
import { Button } from "@/components/ui/button"

type VerdictBadgeProps = {
  verdict?: VerificationVerdict
  running?: boolean
  className?: string
}

const OUTCOME_LABEL: Record<VerificationVerdict["outcome"], string> = {
  pass: "Verified",
  fail: "Needs work",
  inconclusive: "Inconclusive",
}

const outcomeClasses = (outcome: VerificationVerdict["outcome"]): string => {
  if (outcome === "pass") return "text-green"
  if (outcome === "fail") return "text-red"
  return "text-yellow"
}

const outcomeGlyph = (outcome: VerificationVerdict["outcome"]): string => {
  if (outcome === "pass") return "✓"
  if (outcome === "fail") return "✗"
  return "?"
}

export const VerdictBadge = ({
  verdict,
  running = false,
  className,
}: VerdictBadgeProps) => {
  const [expanded, setExpanded] = useState(false)

  if (running || !verdict) {
    return (
      <div className={cn("flex min-h-6 items-center gap-1.5 text-base", className)}>
        <LoaderCircle className="h-3 w-3 shrink-0 animate-spin text-ink-faint" aria-hidden />
        <span className="animate-shimmer-text text-ink-muted">Verifying…</span>
      </div>
    )
  }

  const reason = verdict.findings.join(" ") || "No findings reported."
  const canExpand = verdict.findings.length > 0

  return (
    <div className={cn("flex flex-col", className)}>
      <Button
        variant="ghost"
        onClick={() => canExpand && setExpanded((v) => !v)}
        aria-expanded={expanded}
        disabled={!canExpand}
        title={reason}
        className={cn(
          "group h-auto w-full justify-start gap-1.5 px-0 py-0 font-normal text-base hover:bg-transparent",
          !canExpand && "cursor-default",
        )}
      >
        <span className={cn("shrink-0", outcomeClasses(verdict.outcome))} aria-hidden>
          {outcomeGlyph(verdict.outcome)}
        </span>
        <span className={cn("min-w-0 truncate", outcomeClasses(verdict.outcome))}>
          {OUTCOME_LABEL[verdict.outcome]}
        </span>
        {typeof verdict.confidence === "number" ? (
          <span className="shrink-0 text-ink-faint [font-variant-numeric:tabular-nums]">
            {Math.round(verdict.confidence * 100)}%
          </span>
        ) : null}
        {canExpand ? (
          <ChevronRight
            className={cn(
              "h-2.5 w-2.5 shrink-0 text-icon-3 opacity-0 transition-[transform,opacity] duration-[var(--duration-fast)]",
              "group-hover:opacity-100 group-focus-visible:opacity-100",
              expanded && "rotate-90 opacity-100",
            )}
            aria-hidden
          />
        ) : null}
      </Button>
      <Collapsible open={expanded && canExpand}>
        <ul className="mt-0.5 ml-1.5 flex flex-col gap-0.5 py-0.5 pl-3">
          {verdict.findings.map((finding, i) => (
            <li
              key={i}
              className="text-base leading-[1.5] text-ink-muted"
            >
              {finding}
            </li>
          ))}
        </ul>
      </Collapsible>
    </div>
  )
}

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import type {
  PromptAnnotation,
  PromptSegment,
} from "../../../lib/promptEngineering"
import { cn } from "../../../lib/utils"
import { PromptHoverTip } from "./PromptHoverTip"

type PromptMarksPanelProps = {
  segments: PromptSegment[]
  annotations: PromptAnnotation[]
  onApplyFix: (ann: PromptAnnotation) => void
}

const markClass = (severity: PromptAnnotation["severity"]): string => {
  if (severity === "error") {
    return "rounded-[4px] bg-destructive/10 text-destructive ring-1 ring-destructive/30"
  }
  if (severity === "info") {
    return "rounded-[4px] bg-fill-3 text-ink-secondary ring-1 ring-stroke-3"
  }
  return "rounded-[4px] bg-yellow/15 text-yellow ring-1 ring-yellow/35"
}

/** Annotated prompt view: highlight spans, hover tip, click-to-apply fixes. */
export const PromptMarksPanel = ({
  segments,
  annotations,
  onApplyFix,
}: PromptMarksPanelProps) => {
  const [hoverTip, setHoverTip] = useState<{
    x: number
    y: number
    message: string
    fix?: string
  } | null>(null)

  return (
    <>
      <ScrollArea className="h-full">
        <div
          className={cn(
            "whitespace-pre-wrap break-words",
            "px-2.5 py-2 text-sm leading-relaxed text-ink",
          )}
          onMouseLeave={() => setHoverTip(null)}
        >
        {segments.map((seg, i) =>
          seg.kind === "text" ? (
            <span key={i}>{seg.value}</span>
          ) : (
            <span
              key={i}
              className={cn(
                "cursor-pointer transition-colors duration-[var(--duration-fast)]",
                markClass(seg.severity),
              )}
              onMouseEnter={(e) => {
                const r = e.currentTarget.getBoundingClientRect()
                setHoverTip({
                  x: r.left + r.width / 2,
                  y: r.top,
                  message: seg.message,
                  fix: seg.fix,
                })
              }}
              onMouseLeave={() => setHoverTip(null)}
              onClick={() => {
                const ann = annotations.find(
                  (a) => a.message === seg.message && a.quote === seg.value,
                )
                if (ann?.fix) onApplyFix(ann)
              }}
              title={
                seg.fix
                  ? `${seg.message} — click to apply “${seg.fix}”`
                  : seg.message
              }
            >
              {seg.value}
            </span>
          ),
        )}
        </div>
      </ScrollArea>
      <PromptHoverTip tip={hoverTip} />
    </>
  )
}

type PromptFindingsListProps = {
  annotations: PromptAnnotation[]
  onApplyFix: (ann: PromptAnnotation) => void
  onDismissFinding: (ann: PromptAnnotation) => void
  hasReview: boolean
  hasQuestions: boolean
}

/** Bottom findings list / empty-review status for the prompt pad. */
export const PromptFindingsList = ({
  annotations,
  onApplyFix,
  onDismissFinding,
  hasReview,
  hasQuestions,
}: PromptFindingsListProps) => {
  if (hasReview && annotations.length > 0) {
    return (
      <ScrollArea className="max-h-[36%] shrink-0 border-t border-stroke-3">
        <ul className="flex flex-col gap-0.5 px-2.5 py-2">
          {annotations.map((a, i) => (
            <li
              key={`${a.start}-${a.end}-${i}`}
              className="flex items-start gap-2 rounded-md px-2 py-1.5 text-xs hover:bg-fill-4"
            >
              <span
                className={cn(
                  "mt-0.5 shrink-0 rounded px-1 py-px font-medium uppercase tracking-wide",
                  a.severity === "error" && "bg-destructive/10 text-destructive",
                  a.severity === "warn" && "bg-yellow/15 text-yellow",
                  a.severity === "info" && "bg-muted text-muted-foreground",
                )}
              >
                {a.severity}
              </span>
              <div className="min-w-0 flex-1">
                <p className="text-ink">
                  <span className="font-mono text-ink-secondary">
                    “{a.quote}”
                  </span>
                  {" — "}
                  {a.message}
                </p>
                <div className="mt-0.5 flex flex-wrap gap-2">
                  {a.fix ? (
                    <Button
                      variant="link"
                      onClick={() => onApplyFix(a)}
                      className="h-auto px-0 py-0 text-accent font-normal"
                    >
                      Apply: {a.fix}
                    </Button>
                  ) : null}
                  <Button
                    variant="link"
                    onClick={() => onDismissFinding(a)}
                    className="h-auto px-0 py-0 text-ink-muted font-normal"
                  >
                    Dismiss
                  </Button>
                </div>
              </div>
            </li>
          ))}
        </ul>
      </ScrollArea>
    )
  }

  if (hasReview) {
    return (
      <p className="shrink-0 border-t border-stroke-3 px-2.5 py-2 text-xs text-ink-muted">
        {hasQuestions
          ? "Answer the questions above, or edit the prompt and Verify again."
          : "No open span issues — edit freely or Verify again."}
      </p>
    )
  }

  return (
    <p className="shrink-0 border-t border-stroke-3 px-2.5 py-1.5 text-xs text-ink-faint">
      @ files/MCP · / commands · Verify to grill (apply fixes without ending the
      review).
    </p>
  )
}

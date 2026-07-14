import { useState } from "react"
import { ChevronRight } from "lucide-react"
import { Tooltip } from "../../atoms"
import { Collapsible } from "../../molecules"
import { cn } from "../../../lib/utils"
import { thinkingDurationLabel } from "./buildDisplayItems"

export const ThinkingBlock = ({
  text,
  durationMs,
  streaming,
  suppressStatusLabel = false,
}: {
  text: string
  durationMs?: number
  streaming?: boolean
  /** When true (open live WorkGroup already owns the "Thinking" cue), skip
   * the shimmering status label so the turn shows exactly one live status.
   * While streaming, also skip the chevron-only chrome row — an orphan ▸
   * under tool steps (Glob/Grep "Explored…") reads as a layout bug. Settled
   * blocks still show "Thought for Xs" + expand. */
  suppressStatusLabel?: boolean
}) => {
  const [collapsed, setCollapsed] = useState(true)

  // Empty shells are dropped in `mergeShortThinkingRows`; keep this guard so a
  // stray whitespace-only row never paints a bare "Thought" chevron.
  if (!text.trim()) return null

  // Parent WorkGroup already shows "Thinking" — don't render a chevron-only
  // header (no label left), which floats under the last tool detail.
  if (streaming && suppressStatusLabel) {
    return (
      <div className="min-h-5">
        <p className="whitespace-pre-wrap pb-1 text-base leading-relaxed text-ink-muted opacity-50">
          {text}
        </p>
      </div>
    )
  }

  const statusLabel = streaming ? (
    <span className="animate-shimmer-text">Thinking</span>
  ) : (
    <span>
      Thought{" "}
      {typeof durationMs === "number" ? (
        <span className="text-ink-faint">
          {thinkingDurationLabel(durationMs)}
        </span>
      ) : null}
    </span>
  )

  return (
    <div className="min-h-5">
      <Tooltip label={collapsed ? "Model reasoning — click to expand" : "Click to collapse"}>
        <button
          type="button"
          onClick={() => setCollapsed((v) => !v)}
          aria-expanded={!collapsed}
          className="group flex min-h-5 w-full items-center gap-1.5 text-left text-base text-ink-muted transition-colors duration-[var(--duration-fast)] hover:text-ink-secondary"
        >
          {statusLabel}
          <ChevronRight
            className={cn(
              "h-2.5 w-2.5 text-icon-3 opacity-0 transition-[transform,opacity] duration-[var(--duration-fast)]",
              "group-hover:opacity-100 group-focus-visible:opacity-100",
              !collapsed && "rotate-90 opacity-100",
            )}
            aria-hidden
          />
        </button>
      </Tooltip>
      <Collapsible open={!collapsed}>
        <p className="whitespace-pre-wrap pb-1 text-base leading-relaxed text-ink-muted opacity-50">
          {text}
        </p>
      </Collapsible>
    </div>
  )
}

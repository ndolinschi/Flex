import { memo, useState } from "react"
import { ChevronRight } from "lucide-react"
import { Tooltip } from "../../atoms"
import { Collapsible } from "../../molecules"
import { cn } from "../../../lib/utils"
import { thinkingDurationLabel } from "./buildDisplayItems"
import { Button } from "@/components/ui/button"

export const ThinkingBlock = memo(function ThinkingBlock({
  text,
  durationMs,
  streaming,
  suppressStatusLabel = false,
}: {
  text: string
  durationMs?: number
  streaming?: boolean
  suppressStatusLabel?: boolean
}) {
  const [collapsed, setCollapsed] = useState(true)

  if (!text.trim()) return null

  const displayText =
    streaming && text.length > 4_000 ? text.slice(-4_000) : text

  if (streaming && suppressStatusLabel) {
    return (
      <div className="min-h-5">
        <p className="whitespace-pre-wrap pb-1 text-base leading-relaxed text-ink-muted opacity-50">
          {displayText}
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
        <Button
          variant="ghost"
          onClick={() => setCollapsed((v) => !v)}
          aria-expanded={!collapsed}
          className="group h-auto min-h-5 w-full justify-start gap-1.5 px-0 py-0 font-normal text-base text-ink-muted hover:bg-transparent hover:text-ink-secondary"
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
        </Button>
      </Tooltip>
      <Collapsible open={!collapsed}>
        <p className="whitespace-pre-wrap pb-1 text-base leading-relaxed text-ink-muted opacity-50">
          {displayText}
        </p>
      </Collapsible>
    </div>
  )
})

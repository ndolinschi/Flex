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
}: {
  text: string
  durationMs?: number
  streaming?: boolean
}) => {
  const [collapsed, setCollapsed] = useState(true)

  return (
    <div className="min-h-5">
      <Tooltip label={collapsed ? "Model reasoning — click to expand" : "Click to collapse"}>
        <button
          type="button"
          onClick={() => setCollapsed((v) => !v)}
          aria-expanded={!collapsed}
          className="group flex min-h-5 w-full items-center gap-1.5 text-left text-base text-ink-muted transition-colors hover:text-ink-secondary"
        >
          {streaming ? (
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
          )}
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


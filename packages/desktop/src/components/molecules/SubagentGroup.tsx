import { useState, type ReactNode } from "react"
import { ChevronRight, Bot } from "lucide-react"
import { cn, formatDuration } from "../../lib/utils"
import { Collapsible } from "./Collapsible"

type SubagentGroupProps = {
  task: string
  role?: string
  phase: "started" | "completed"
  durationMs?: number
  children: ReactNode
}

/** Collapsible nested subagent work block (Cursor-style). */
export const SubagentGroup = ({
  task,
  role,
  phase,
  durationMs,
  children,
}: SubagentGroupProps) => {
  const [expanded, setExpanded] = useState(phase === "started")
  const open = phase === "started" || expanded

  return (
    <div className="flex flex-col pl-1">
      <button
        type="button"
        onClick={() => {
          if (phase === "started") return
          setExpanded((v) => !v)
        }}
        aria-expanded={open}
        className={cn(
          "group flex min-h-7 w-full items-center gap-1.5 text-left text-base",
          phase !== "started" && "cursor-pointer",
        )}
      >
        <Bot className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
        <span className="min-w-0 truncate text-ink-secondary">
          {role ? `${role}: ` : ""}
          {task}
        </span>
        {phase === "completed" && typeof durationMs === "number" ? (
          <span className="shrink-0 text-ink-muted [font-variant-numeric:tabular-nums]">
            {formatDuration(durationMs)}
          </span>
        ) : null}
        {phase === "completed" ? (
          <ChevronRight
            className={cn(
              "h-2.5 w-2.5 text-icon-3 opacity-0 transition-[transform,opacity] duration-[var(--duration-fast)]",
              "group-hover:opacity-100",
              open && "rotate-90 opacity-100",
            )}
            aria-hidden
          />
        ) : null}
      </button>
      <Collapsible open={open && !!children}>
        <div className="ml-1.5 flex flex-col gap-1 border-l border-stroke-3 py-1 pl-3">
          {children}
        </div>
      </Collapsible>
    </div>
  )
}

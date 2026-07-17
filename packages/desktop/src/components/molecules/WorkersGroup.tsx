import { memo, useEffect, useRef, useState } from "react"
import { Bot, ChevronRight, Network } from "@/components/icons"
import type { SubagentTimelineRow } from "../../lib/workerPresentation"
import { workersHeaderLabel } from "../../lib/workerPresentation"
import { cn } from "../../lib/utils"
import { Collapsible } from "./Collapsible"
import { SubagentGroup } from "./SubagentGroup"

type WorkersGroupProps = {
  workers: SubagentTimelineRow[]
  /** Opens SubagentViewer for a child session. */
  onOpenViewer: (sessionId: string, title: string) => void
  /** Anchor id for scroll-from-pill. */
  anchorId?: string
}

/** Parent card for parallel Agent fan-out: one "Working with N agents"
 * header that expands to enriched worker rows. */
export const WorkersGroup = memo(function WorkersGroup({
  workers,
  onOpenViewer,
  anchorId,
}: WorkersGroupProps) {
  const anyRunning = workers.some((w) => w.phase === "started")
  const [expanded, setExpanded] = useState(true)
  const open = anyRunning || expanded
  const prevRunning = useRef(anyRunning)

  useEffect(() => {
    if (prevRunning.current !== anyRunning) {
      if (anyRunning) setExpanded(true)
      else if (prevRunning.current) setExpanded(false)
      prevRunning.current = anyRunning
    }
  }, [anyRunning])

  const openWorker = (w: SubagentTimelineRow) => {
    if (!w.childSession) return
    onOpenViewer(
      w.childSession,
      `${w.role ? `${w.role} — ` : ""}${w.task.split("\n", 1)[0]}`,
    )
  }

  // Single worker: no outer chrome — SubagentGroup already carries status.
  if (workers.length === 1) {
    const w = workers[0]
    return (
      <div id={anchorId} data-workers-group className="flex flex-col pl-1">
        <SubagentGroup
          task={w.task}
          role={w.role}
          phase={w.phase}
          durationMs={w.summary?.duration_ms}
          summary={w.summary}
          nestedRows={w.children}
          compact
          onOpenViewer={
            w.childSession ? () => openWorker(w) : undefined
          }
        />
      </div>
    )
  }

  const label = workersHeaderLabel(workers)

  return (
    <div id={anchorId} data-workers-group className="flex flex-col pl-1">
      <button
        type="button"
        onClick={() => {
          if (anyRunning) return
          setExpanded((v) => !v)
        }}
        aria-expanded={open}
        className={cn(
          "group flex min-h-7 w-full items-center gap-1.5 text-left text-base",
          !anyRunning && "cursor-pointer",
        )}
      >
        <Network className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
        <span
          className={cn(
            "min-w-0 truncate text-ink-secondary",
            anyRunning && "animate-shimmer-text",
          )}
        >
          {label}
        </span>
        {!anyRunning ? (
          <ChevronRight
            className={cn(
              "h-2.5 w-2.5 shrink-0 text-icon-3 opacity-0 transition-[transform,opacity] duration-[var(--duration-fast)]",
              "group-hover:opacity-100",
              open && "rotate-90 opacity-100",
            )}
            aria-hidden
          />
        ) : (
          <Bot className="h-3 w-3 shrink-0 text-ink-faint" aria-hidden />
        )}
      </button>
      <Collapsible open={open}>
        <div className="ml-1.5 flex flex-col gap-0.5 border-l border-stroke-3 py-1 pl-3">
          {workers.map((w) => (
            <SubagentGroup
              key={w.childSession}
              task={w.task}
              role={w.role}
              phase={w.phase}
              durationMs={w.summary?.duration_ms}
              summary={w.summary}
              nestedRows={w.children}
              compact
              onOpenViewer={
                w.childSession ? () => openWorker(w) : undefined
              }
            />
          ))}
        </div>
      </Collapsible>
    </div>
  )
})

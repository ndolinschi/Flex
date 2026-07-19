import { useEffect, useRef, useState } from "react"
import { Bot, ChevronDown, Network } from "lucide-react"
import type { SubagentTimelineRow } from "../../lib/workerPresentation"
import {
  summarizeWorkerActivity,
  workerTitle,
} from "../../lib/workerPresentation"
import { cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import { PopoverTray } from "./PopoverTray"

type WorkingAgentsPillProps = {
  workers: SubagentTimelineRow[]
  /** Scroll the timeline to the active workers group. */
  onScrollToWorkers?: () => void
}

/** Composer-adjacent glance: "N Working" opens a short menu of running
 * worker titles (open viewer / scroll to group). Hidden when none running. */
export const WorkingAgentsPill = ({
  workers,
  onScrollToWorkers,
}: WorkingAgentsPillProps) => {
  const openSubagentViewer = useAppStore((s) => s.openSubagentViewer)
  const [open, setOpen] = useState(false)
  const anchorRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    if (workers.length === 0) setOpen(false)
  }, [workers.length])

  if (workers.length === 0) return null

  const n = workers.length
  const label = n === 1 ? "1 Working" : `${n} Working`

  return (
    <div className="relative mb-1.5 flex justify-start">
      <button
        ref={anchorRef}
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        aria-haspopup="menu"
        className={cn(
          "inline-flex h-6 items-center gap-1.5 rounded-md border border-stroke-3",
          "bg-fill-3 px-2 text-sm text-ink-secondary",
          "transition-colors duration-[var(--duration-fast)] hover:bg-fill-4 hover:text-ink",
        )}
      >
        <Network className="h-3 w-3 shrink-0 text-ink-faint" aria-hidden />
        <span className="animate-shimmer-text">{label}</span>
        <ChevronDown
          className={cn(
            "h-2.5 w-2.5 text-icon-3 transition-transform duration-[var(--duration-fast)]",
            open && "rotate-180",
          )}
          aria-hidden
        />
      </button>
      <PopoverTray
        open={open}
        onClose={() => setOpen(false)}
        placement="above"
        anchorRef={anchorRef}
        role="menu"
        aria-label="Running workers"
        className="left-0 min-w-[220px] max-w-[320px] border border-stroke-3"
      >
        <div className="flex flex-col gap-0.5 p-1">
          {onScrollToWorkers ? (
            <button
              type="button"
              role="menuitem"
              className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-ink-secondary hover:bg-fill-4"
              onClick={() => {
                setOpen(false)
                onScrollToWorkers()
              }}
            >
              <Network className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
              Jump to workers
            </button>
          ) : null}
          {workers.map((w) => {
            const activity = summarizeWorkerActivity(
              w.children,
              w.phase,
              w.summary,
            )
            const title = workerTitle(w.role, w.task)
            return (
              <button
                key={w.childSession}
                type="button"
                role="menuitem"
                className="flex w-full flex-col gap-0.5 rounded-md px-2 py-1.5 text-left hover:bg-fill-4"
                onClick={() => {
                  setOpen(false)
                  openSubagentViewer(w.childSession, title)
                }}
              >
                <span className="flex min-w-0 items-center gap-1.5 text-sm text-ink-secondary">
                  <Bot className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
                  <span className="min-w-0 truncate">{title}</span>
                </span>
                {activity.latestLabel ? (
                  <span className="truncate pl-5 text-sm text-ink-faint">
                    {activity.latestLabel}
                  </span>
                ) : null}
              </button>
            )
          })}
        </div>
      </PopoverTray>
    </div>
  )
}

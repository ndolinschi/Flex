import { useEffect, useState } from "react"
import { Bot, Network, X } from "lucide-react"
import type { SubagentTimelineRow } from "../../lib/workerPresentation"
import {
  summarizeWorkerActivity,
  workerTitle,
} from "../../lib/workerPresentation"
import { useAppStore } from "../../stores/appStore"
import { Button } from "@/components/ui/button"
import { cn } from "../../lib/utils"

type WorkingAgentsPillProps = {
  workers: SubagentTimelineRow[]
  onScrollToWorkers?: () => void
  /** Cancel the parent turn (and thus running workers) — Cursor “Stop All”. */
  onStopAll?: () => void
}

export const WorkingAgentsPill = ({
  workers,
  onScrollToWorkers,
  onStopAll,
}: WorkingAgentsPillProps) => {
  const openSubagentViewer = useAppStore((s) => s.openSubagentViewer)
  const [stopping, setStopping] = useState(false)

  useEffect(() => {
    if (workers.length === 0) setStopping(false)
  }, [workers.length])

  if (workers.length === 0) return null

  const n = workers.length
  const preview = workers.slice(0, 4)
  const overflow = n - preview.length

  const handleStopAll = () => {
    if (!onStopAll || stopping) return
    setStopping(true)
    onStopAll()
  }

  return (
    <div
      className={cn(
        "relative mb-1.5 overflow-hidden rounded-[var(--radius-composer)]",
        "border border-stroke-3 bg-elevated/90",
      )}
    >
      <div className="flex h-7 items-center gap-2 px-2.5">
        <Network className="size-3.5 shrink-0 text-ink-muted" aria-hidden />
        <button
          type="button"
          className={cn(
            "min-w-0 flex-1 truncate text-left text-sm text-ink",
            "animate-shimmer-text",
            onScrollToWorkers &&
              "hover:text-ink focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-stroke-2",
          )}
          onClick={onScrollToWorkers}
          disabled={!onScrollToWorkers}
        >
          {n} Working
        </button>
        {onStopAll ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            aria-label="Stop all workers"
            disabled={stopping}
            onClick={handleStopAll}
            className="h-6 gap-1 px-1.5 text-xs text-ink-muted hover:bg-fill-4 hover:text-ink"
          >
            <X className="size-3" aria-hidden />
            Stop All
          </Button>
        ) : null}
      </div>
      <ul className="flex flex-col gap-0.5 border-t border-stroke-3 px-2.5 py-1.5">
        {preview.map((w) => {
          const activity = summarizeWorkerActivity(
            w.children,
            w.phase,
            w.summary,
          )
          const title = workerTitle(w.role, w.task)
          return (
            <li key={w.childSession}>
              <button
                type="button"
                className={cn(
                  "flex w-full min-w-0 items-start gap-2 rounded-md px-1 py-1 text-left",
                  "transition-colors duration-[var(--duration-fast)]",
                  "hover:bg-fill-4 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-stroke-2",
                )}
                onClick={() => openSubagentViewer(w.childSession, title)}
              >
                <Bot
                  className="mt-0.5 size-3 shrink-0 text-ink-faint"
                  aria-hidden
                />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-xs text-ink">
                    {title}
                  </span>
                  {activity.latestLabel ? (
                    <span className="block truncate text-xs text-ink-faint">
                      {activity.latestLabel}
                    </span>
                  ) : null}
                </span>
              </button>
            </li>
          )
        })}
        {overflow > 0 && onScrollToWorkers ? (
          <li>
            <button
              type="button"
              className="w-full px-1 py-0.5 text-left text-xs text-ink-faint hover:text-ink-muted"
              onClick={onScrollToWorkers}
            >
              +{overflow} more — jump to workers
            </button>
          </li>
        ) : null}
      </ul>
    </div>
  )
}

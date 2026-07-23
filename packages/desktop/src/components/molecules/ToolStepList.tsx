import { memo, useMemo, type ReactNode } from "react"
import { ToolStepGroup } from "./ToolStepGroup"
import {
  isRunning,
  type TimelineToolRowLike,
} from "../../lib/toolPresentation"
import {
  clusterWorkRows,
  type SubagentTimelineRow,
} from "../../lib/workerPresentation"
import {
  progressForRunningCalls,
  windowToolCalls,
} from "../../lib/timeline/windowToolRows"
import { WorkersGroup } from "./WorkersGroup"
import { useAppStore } from "../../stores/appStore"

export const ToolStepList = memo(function ToolStepList({
  rows,
  renderOther,
  progress,
}: {
  rows: TimelineToolRowLike[]
  renderOther: (row: TimelineToolRowLike) => ReactNode
  progress?: Record<string, string>
  forceOpenDetails?: boolean
}) {
  const openSubagentViewer = useAppStore((s) => s.openSubagentViewer)
  const clusters = useMemo(() => clusterWorkRows(rows), [rows])
  return (
    <>
      {clusters.map((cluster, i) => {
        if (cluster.kind === "tools") {
          const windowed = windowToolCalls(cluster.calls)
          const slimProgress = progressForRunningCalls(
            windowed.calls,
            progress,
          )
          return (
            <div key={`tools:${cluster.calls[0].id}`}>
              {windowed.earlierCount > 0 ? (
                <p className="min-h-[var(--timeline-row-min-height)] px-0 py-px text-base leading-[1.5] text-ink-muted/70">
                  {windowed.earlierCount} earlier step
                  {windowed.earlierCount === 1 ? "" : "s"}
                </p>
              ) : null}
              <ToolStepGroup
                calls={windowed.calls}
                forceOpen={windowed.calls.some(isRunning)}
                progress={slimProgress}
              />
            </div>
          )
        }
        if (cluster.kind === "workers") {
          const workers = cluster.workers as SubagentTimelineRow[]
          return (
            <WorkersGroup
              key={`workers:${workers[0]?.childSession ?? i}`}
              workers={workers}
              anchorId={
                workers.some((w) => w.phase === "started")
                  ? "active-workers-group"
                  : undefined
              }
              onOpenViewer={openSubagentViewer}
            />
          )
        }
        return (
          <div
            key={cluster.row.id || `other-${i}`}
            className="animate-tool-step-in"
          >
            {renderOther(cluster.row)}
          </div>
        )
      })}
    </>
  )
})

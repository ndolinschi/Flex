import { useMemo, type ReactNode } from "react"
import { ToolStepGroup } from "./ToolStepGroup"
import {
  isRunning,
  type TimelineToolRowLike,
} from "../../lib/toolPresentation"
import {
  clusterWorkRows,
  type SubagentTimelineRow,
} from "../../lib/workerPresentation"
import { WorkersGroup } from "./WorkersGroup"
import { useAppStore } from "../../stores/appStore"

export const ToolStepList = ({
  rows,
  renderOther,
  progress,
}: {
  rows: TimelineToolRowLike[]
  renderOther: (row: TimelineToolRowLike) => ReactNode
  progress?: Record<string, string>
  forceOpenDetails?: boolean
}) => {
  const openSubagentViewer = useAppStore((s) => s.openSubagentViewer)
  const clusters = useMemo(() => clusterWorkRows(rows), [rows])
  return (
    <>
      {clusters.map((cluster, i) => {
        if (cluster.kind === "tools") {
          return (
            <ToolStepGroup
              key={`tools:${cluster.calls[0].id}`}
              calls={cluster.calls}
              forceOpen={cluster.calls.some(isRunning)}
              progress={progress}
            />
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
}

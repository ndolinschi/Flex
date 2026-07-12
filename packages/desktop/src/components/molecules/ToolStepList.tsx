import type { ReactNode } from "react"
import { ToolStepGroup } from "./ToolStepGroup"
import {
  clusterToolRows,
  isRunning,
  type TimelineToolRowLike,
} from "../../lib/toolPresentation"

export const ToolStepList = ({
  rows,
  renderOther,
  progress,
  forceOpenDetails = false,
}: {
  rows: TimelineToolRowLike[]
  renderOther: (row: TimelineToolRowLike) => ReactNode
  progress?: Record<string, string>
  /** Keep every tool-step cluster expanded (live turn) — not only while a
   * call inside that cluster is still running. */
  forceOpenDetails?: boolean
}) => {
  const clusters = clusterToolRows(rows)
  return (
    <>
      {clusters.map((cluster, i) =>
        cluster.kind === "tools" ? (
          <ToolStepGroup
            // Stable across the cluster's lifetime: keyed on the FIRST call's
            // id only, not the full (growing) id list. Keying on the joined
            // list meant every new call appended to a running cluster changed
            // the key, forcing a full unmount/remount (expanded state reset,
            // DOM subtree replaced) instead of an in-place update.
            key={`tools:${cluster.calls[0].id}`}
            calls={cluster.calls}
            forceOpen={forceOpenDetails || cluster.calls.some(isRunning)}
            progress={progress}
          />
        ) : (
          <div
            key={cluster.row.id || `other-${i}`}
            className="animate-tool-step-in"
          >
            {renderOther(cluster.row)}
          </div>
        ),
      )}
    </>
  )
}

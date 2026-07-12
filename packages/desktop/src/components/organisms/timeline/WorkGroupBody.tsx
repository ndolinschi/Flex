import { useCallback, useRef } from "react"
import { ToolStepList } from "../../molecules"
import type { TimelineToolRowLike } from "../../../lib/toolPresentation"
import type { TimelineRow } from "../../../lib/types"
import { TimelineRowView } from "./TimelineRowView"

type WorkGroupBodyProps = {
  rows: TimelineRow[]
  progress?: Record<string, string>
  /** Keep tool-step clusters expanded (live open turn). */
  forceOpenDetails: boolean
  thinkingDurations?: Record<string, number>
  sessionId: string
  checkpointsDisabled: boolean
}

/**
 * Owns a stable `renderOther` for `ToolStepList` so `memo(TimelineRowView)`
 * can skip settled rows while the parent timeline re-renders on streaming
 * deltas. Volatile props (`thinkingDurations`, `checkpointsDisabled`) are
 * read through refs at call time — they must not appear in the
 * `useCallback` dep list or every rAF flush would recreate `renderOther`
 * and defeat the memo.
 */
export const WorkGroupBody = ({
  rows,
  progress,
  forceOpenDetails,
  thinkingDurations,
  sessionId,
  checkpointsDisabled,
}: WorkGroupBodyProps) => {
  const thinkingDurationsRef = useRef(thinkingDurations)
  thinkingDurationsRef.current = thinkingDurations
  const checkpointsDisabledRef = useRef(checkpointsDisabled)
  checkpointsDisabledRef.current = checkpointsDisabled

  const renderOther = useCallback(
    (row: TimelineToolRowLike) => (
      <TimelineRowView
        row={row as TimelineRow}
        thinkingDurations={thinkingDurationsRef.current}
        sessionId={sessionId}
        checkpointsDisabled={checkpointsDisabledRef.current}
        suppressThinkingStatusLabel={forceOpenDetails}
      />
    ),
    [sessionId, forceOpenDetails],
  )

  return (
    <ToolStepList
      rows={rows}
      progress={progress}
      forceOpenDetails={forceOpenDetails}
      renderOther={renderOther}
    />
  )
}

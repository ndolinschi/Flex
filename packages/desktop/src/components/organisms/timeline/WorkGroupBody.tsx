import { useCallback, useRef } from "react"
import { ToolStepList } from "../../molecules"
import type { TimelineToolRowLike } from "../../../lib/toolPresentation"
import type { TimelineRow } from "../../../lib/types"
import { TimelineRowView } from "./TimelineRowView"

type WorkGroupBodyProps = {
  rows: TimelineRow[]
  progress?: Record<string, string>
  forceOpenDetails: boolean
  thinkingDurations?: Record<string, number>
  sessionId: string
  checkpointsDisabled: boolean
}

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

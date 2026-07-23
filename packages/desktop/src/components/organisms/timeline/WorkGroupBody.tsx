import { memo, useCallback, useMemo, useRef } from "react"
import { ToolStepList } from "../../molecules"
import type { TimelineToolRowLike } from "../../../lib/toolPresentation"
import type { TimelineRow } from "../../../lib/types"
import { windowWorkGroupRows } from "../../../lib/timeline/windowToolRows"
import { TimelineRowView } from "./TimelineRowView"

type WorkGroupBodyProps = {
  rows: TimelineRow[]
  progress?: Record<string, string>
  forceOpenDetails: boolean
  thinkingDurations?: Record<string, number>
  sessionId: string
  checkpointsDisabled: boolean
}

export const WorkGroupBody = memo(function WorkGroupBody({
  rows,
  progress,
  forceOpenDetails,
  thinkingDurations,
  sessionId,
  checkpointsDisabled,
}: WorkGroupBodyProps) {
  const thinkingDurationsRef = useRef(thinkingDurations)
  thinkingDurationsRef.current = thinkingDurations
  const checkpointsDisabledRef = useRef(checkpointsDisabled)
  checkpointsDisabledRef.current = checkpointsDisabled

  const windowed = useMemo(() => windowWorkGroupRows(rows), [rows])

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
    <>
      {windowed.earlierCount > 0 ? (
        <p className="min-h-[var(--timeline-row-min-height)] px-0 py-px text-base leading-[1.5] text-ink-muted/70">
          {windowed.earlierCount} earlier step
          {windowed.earlierCount === 1 ? "" : "s"}
        </p>
      ) : null}
      <ToolStepList
        rows={windowed.rows}
        progress={progress}
        forceOpenDetails={forceOpenDetails}
        renderOther={renderOther}
      />
    </>
  )
})

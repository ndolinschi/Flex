import { memo } from "react"
import type { ToolCall } from "../../lib/types"
import { ToolStepGroup } from "./ToolStepGroup"

type ToolCallChipProps = {
  call: ToolCall
  className?: string
}

/** Single tool call — renders as a step (also used for live streaming). */
export const ToolCallChip = memo(function ToolCallChip({
  call,
  className,
}: ToolCallChipProps) {
  return <ToolStepGroup calls={[call]} className={className} />
})

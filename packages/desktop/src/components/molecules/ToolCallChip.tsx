import type { ToolCall } from "../../lib/types"
import { ToolStepGroup } from "./ToolStepGroup"

type ToolCallChipProps = {
  call: ToolCall
  className?: string
}

/** Single tool call — renders as a Cursor-style step (also used for live streaming). */
export const ToolCallChip = ({ call, className }: ToolCallChipProps) => (
  <ToolStepGroup calls={[call]} className={className} />
)

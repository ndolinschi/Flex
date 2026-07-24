import type { TimelineRow } from "../types"

/**
 * Prefer scanning from the end: live tool/workflow/verdict updates almost
 * always hit recently appended rows, so average cost is O(k) near the tip
 * instead of a full O(n) forward findIndex.
 */
export const findRowIndexFromEnd = (
  rows: readonly TimelineRow[],
  pred: (row: TimelineRow, index: number) => boolean,
): number => {
  for (let i = rows.length - 1; i >= 0; i--) {
    if (pred(rows[i], i)) return i
  }
  return -1
}

export const findToolRowIndex = (
  rows: readonly TimelineRow[],
  callId: string,
): number =>
  findRowIndexFromEnd(
    rows,
    (r) => r.type === "tool" && r.call.id === callId,
  )

export const findVerdictRowIndex = (
  rows: readonly TimelineRow[],
  callId: string,
): number =>
  findRowIndexFromEnd(
    rows,
    (r) => r.type === "verdict" && r.callId === callId,
  )

export const findWorkflowRowIndex = (
  rows: readonly TimelineRow[],
  callId: string,
): number =>
  findRowIndexFromEnd(
    rows,
    (r) => r.type === "workflow" && r.callId === callId,
  )

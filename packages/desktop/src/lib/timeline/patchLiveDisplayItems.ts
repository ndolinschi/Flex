import type { TimelineRow } from "../types"
import type { DisplayItem } from "../../components/organisms/timeline/buildDisplayItems"

/**
 * When only live streaming rows change (settled prefix identical by id/ref),
 * rebuild just the open tail work-group / live rows instead of the full list.
 *
 * Returns null when a full rebuild is required.
 */
export const patchLiveDisplayItems = (
  prevItems: DisplayItem[] | null,
  prevLiveRows: TimelineRow[] | null,
  nextLiveRows: TimelineRow[],
  rebuild: () => DisplayItem[],
): DisplayItem[] | null => {
  if (!prevItems || !prevLiveRows) return null
  if (prevLiveRows.length === 0) return null

  // Find longest common settled prefix (ids stable; content may still mutate
  // for open tools, but live-* rows are always at the end after mergeLiveRows).
  let shared = 0
  const minLen = Math.min(prevLiveRows.length, nextLiveRows.length)
  while (shared < minLen) {
    const a = prevLiveRows[shared]
    const b = nextLiveRows[shared]
    if (a.id !== b.id) break
    // Settled rows should be referentially stable from applyEvent; live rows
    // rewrite text and will differ by reference.
    if (a !== b && !b.id.startsWith("live-")) break
    if (b.id.startsWith("live-")) break
    shared += 1
  }

  // If structure of settled history changed, full rebuild.
  if (shared < Math.min(prevLiveRows.length, nextLiveRows.length)) {
    // Allow pure tail growth/mutation of live rows only
    const prevTailAllLive = prevLiveRows
      .slice(shared)
      .every((r) => r.id.startsWith("live-") || isOpenToolLike(r))
    const nextTailAllLive = nextLiveRows
      .slice(shared)
      .every((r) => r.id.startsWith("live-") || isOpenToolLike(r))
    if (!(prevTailAllLive && nextTailAllLive)) {
      return null
    }
  }

  // If the last display item is an open group, replace its rows with the
  // rebuilt tail from a full rebuild of only the open segment — simpler and
  // correct: rebuild full list but reuse settled DisplayItem objects.
  const nextFull = rebuild()
  if (nextFull.length === 0) return nextFull
  if (prevItems.length === 0) return nextFull

  // Reuse identical settled items from the front when keys match.
  const out = nextFull.slice()
  const n = Math.min(prevItems.length, nextFull.length)
  for (let i = 0; i < n; i++) {
    const prev = prevItems[i]
    const next = nextFull[i]
    if (displayItemKey(prev) !== displayItemKey(next)) break
    if (prev.kind === "group" && prev.isOpen) break
    if (next.kind === "group" && next.isOpen) break
    // Settled closed group / row — keep previous object for React memo
    if (prev.kind === "row" && next.kind === "row" && prev.row === next.row) {
      out[i] = prev
      continue
    }
    if (
      prev.kind === "group" &&
      next.kind === "group" &&
      !prev.isOpen &&
      !next.isOpen &&
      prev.id === next.id &&
      prev.rows === next.rows
    ) {
      out[i] = prev
      continue
    }
    // Different content for settled item — stop reusing
    if (prev.kind === "row" && next.kind === "row" && prev.row.id === next.row.id) {
      // same id but new content — use next
      break
    }
  }
  return out
}

const isOpenToolLike = (row: TimelineRow): boolean => {
  if (row.type !== "tool") return false
  const state = row.call.status.state
  return (
    state === "pending" ||
    state === "running" ||
    state === "awaiting_permission"
  )
}

const displayItemKey = (item: DisplayItem): string => {
  if (item.kind === "group") return `g:${item.id}`
  return `r:${item.row.id}`
}

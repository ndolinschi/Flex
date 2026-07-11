/**
 * Singleton tail-buffer bus for live mini-logs under running command rows.
 *
 * Mirrors the shape of terminalBus.ts (per-key buffer + fan-out), but keeps
 * only a small bounded tail per call id — this feeds the compact "last few
 * lines" preview rendered directly in the chat feed, not the full agent
 * terminal scrollback (that's terminalBus's job). Module-level singleton, no
 * React imports beyond types, so it can be read via useSyncExternalStore from
 * any number of ToolStepGroup rows without prop drilling.
 *
 * Buffers are intentionally NOT cleared when a call completes — the mini-log
 * stays visible (muted) under the finished row for the rest of the chat
 * session. See `MAX_TAIL_BUFFERS` for the resulting memory bound.
 *
 * `pushExecTail` also feeds `execErrorScan` (see that module) on every chunk —
 * same call site as the tail buffer append, so error detection stays in sync
 * with the mini-log without `useGlobalSessionEvents` needing to know about it.
 */

import { scanExecChunk } from "./execErrorScan"

type TailSubscriber = () => void

const MAX_TAIL_CHARS = 2_000

/** Tails now persist past call completion (so the mini-log stays visible in
 * the finished chat turn), so the map is capped by call count instead of
 * being drained as calls finish. `Map` iteration/insertion order is the same
 * thing in JS, so the first key in `tails.keys()` is always the
 * oldest-inserted buffer — evicting it is a plain FIFO over call ids. Worst
 * case ~2KB * 50 = ~100KB, negligible. */
const MAX_TAIL_BUFFERS = 50

const tails = new Map<string, string>()
const subscribers = new Map<string, Set<TailSubscriber>>()

const notify = (callId: string) => {
  const subs = subscribers.get(callId)
  if (subs) {
    for (const sub of subs) sub()
  }
}

/** Evict the oldest-inserted buffer(s) until we're back under the cap.
 * Only drops entries with no live subscribers — a buffer currently rendered
 * on screen should never be evicted out from under it. */
const evictOldestIfNeeded = () => {
  if (tails.size <= MAX_TAIL_BUFFERS) return
  for (const key of tails.keys()) {
    if (tails.size <= MAX_TAIL_BUFFERS) break
    if (subscribers.get(key)?.size) continue
    tails.delete(key)
  }
}

/** Append text to a call's tail buffer, keeping only the trailing slice on
 * overflow (oldest output is dropped first — callers only need recent lines).
 * Also feeds the same chunk to `execErrorScan` so error detection tracks the
 * tail buffer without a second call site in `useGlobalSessionEvents`. */
export const pushExecTail = (callId: string, text: string): void => {
  const prev = tails.get(callId) ?? ""
  tails.set(callId, (prev + text).slice(-MAX_TAIL_CHARS))
  evictOldestIfNeeded()
  scanExecChunk(callId, text)
  notify(callId)
}

/** Current buffered tail for a call id, or empty string if none seen yet. */
export const getExecTail = (callId: string): string => tails.get(callId) ?? ""

/** Drop a call's buffer. Tails now persist past completion by default (see
 * module doc), so this is no longer called on the running→done transition —
 * kept for explicit/manual cleanup (e.g. tests) and the eviction path above. */
export const clearExecTail = (callId: string): void => {
  tails.delete(callId)
  subscribers.delete(callId)
}

/** Subscribe to changes for a single call id. Returns an unsubscribe fn.
 * Compatible with React's `useSyncExternalStore` (pair with `getExecTail`). */
export const subscribeExecTail = (
  callId: string,
  onChange: TailSubscriber,
): (() => void) => {
  let subs = subscribers.get(callId)
  if (!subs) {
    subs = new Set()
    subscribers.set(callId, subs)
  }
  subs.add(onChange)
  return () => {
    subs?.delete(onChange)
  }
}

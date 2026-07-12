/**
 * Per-call error detection over exec output.
 *
 * Fed by `execTailBus.pushExecTail` (same call site as the mini-log tail —
 * see that module's doc), so every `exec_chunk` chunk that flows into a
 * call's tail buffer is also scanned here for error signatures. Kept as its
 * own module (no React imports, no dependency on execTailBus) so either side
 * can be tested or reused independently — execTailBus just calls into
 * `scanExecChunk` alongside its own buffer append.
 *
 * Deliberately simple: this is a signal for "something looked like an error
 * in this command's output", not a parser. False negatives (a real error that
 * doesn't match any pattern) are fine; false positives on very common strings
 * ("0 errors", "no errors found") are guarded against explicitly since those
 * show up constantly in clean build output.
 */

/** Strong signatures that a line is reporting an actual error, not just
 * mentioning the word in passing. Case-sensitive where the tool's own output
 * is reliably cased (compiler/runtime prefixes); case-insensitive for the
 * generic "error" word boundary since callers/tools vary in casing. */
const ERROR_PATTERNS: RegExp[] = [
  /\berror(\[|:|\s)/i,
  /Failed to resolve/,
  /Cannot find/,
  /Traceback \(most recent call last\)/,
  /panicked at/,
  /SyntaxError/,
  /TypeError:/,
  /ReferenceError:/,
  /ERR_/,
  /npm ERR!/,
  /Compilation failed/,
  /FAILED/,
]

/** "0 errors" / "no errors" (and plural forms) must never count as an error
 * line — these are exactly the clean-build summaries that would otherwise
 * trip `/\berror(\[|:|\s)/i` above. Checked before the strong patterns. */
const BENIGN_COUNT_RE = /\b(?:0|no)\s+errors?\b/i

/** Last N matched lines kept per call — enough for a useful "fix this" prompt
 * without the composer prefill ballooning. */
const MAX_ERROR_LINES = 10

export type ExecErrorScan = {
  count: number
  /** Last ~10 error-context lines seen for this call, oldest first. */
  lines: string[]
}

type ErrorScanSubscriber = () => void

const errorStates = new Map<string, ExecErrorScan>()
const subscribers = new Map<string, Set<ErrorScanSubscriber>>()

const notify = (callId: string) => {
  const subs = subscribers.get(callId)
  if (subs) {
    for (const sub of subs) sub()
  }
}

/** Subscribe to error-scan changes for a single call id. Returns an
 * unsubscribe fn. Compatible with `useSyncExternalStore` (pair with
 * `getExecErrorScan`), mirroring `subscribeExecTail`. */
export const subscribeExecErrorScan = (
  callId: string,
  onChange: ErrorScanSubscriber,
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

const isBenignLine = (line: string): boolean => BENIGN_COUNT_RE.test(line)

const lineHasError = (line: string): boolean => {
  if (isBenignLine(line)) return false
  return ERROR_PATTERNS.some((re) => re.test(line))
}

/** Scan a freshly-arrived chunk of exec output for a call, updating that
 * call's running error count/tail. `text` may contain multiple lines (or a
 * partial line) — split defensively rather than assuming chunk boundaries
 * align with newlines.
 *
 * Builds a brand-new `ExecErrorScan` object only once per call (the first
 * matched line), then mutates it in place for subsequent matches — the
 * object identity stored in `errorStates` only changes when something
 * actually changed, which is required for `useSyncExternalStore` (its
 * `getSnapshot` must return a referentially-stable value between renders
 * when nothing changed, or React throws "getSnapshot should be cached"). */
export const scanExecChunk = (callId: string, text: string): void => {
  if (!text) return
  const lines = text.split("\n").filter((l) => l.length > 0)
  if (!lines.length) return

  let state = errorStates.get(callId)
  let changed = false
  for (const line of lines) {
    if (!lineHasError(line)) continue
    if (!state) {
      state = { count: 0, lines: [] }
      errorStates.set(callId, state)
    }
    state.count += 1
    state.lines.push(line)
    if (state.lines.length > MAX_ERROR_LINES) {
      state.lines = state.lines.slice(-MAX_ERROR_LINES)
    }
    changed = true
  }
  if (changed) notify(callId)
}

/** Current error-scan result for a call, or `null` if no errors detected yet.
 * Returns the same object reference across calls until the scan state for
 * `callId` actually changes (see `scanExecChunk`) — required for
 * `useSyncExternalStore`. */
export const getExecErrorScan = (callId: string): ExecErrorScan | null => {
  const state = errorStates.get(callId)
  if (!state || state.count === 0) return null
  return state
}

/** Drop a call's error-scan state. Mirrors `clearExecTail` — not called on
 * the running→done transition (scans persist alongside the tail so the badge
 * stays visible on completed rows), kept for explicit/manual cleanup. */
export const clearExecErrorScan = (callId: string): void => {
  errorStates.delete(callId)
  subscribers.delete(callId)
}


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

const BENIGN_COUNT_RE = /\b(?:0|no)\s+errors?\b/i

const MAX_ERROR_LINES = 10

export type ExecErrorScan = {
  count: number
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

export const getExecErrorScan = (callId: string): ExecErrorScan | null => {
  const state = errorStates.get(callId)
  if (!state || state.count === 0) return null
  return state
}

export const clearExecErrorScan = (callId: string): void => {
  errorStates.delete(callId)
  subscribers.delete(callId)
}

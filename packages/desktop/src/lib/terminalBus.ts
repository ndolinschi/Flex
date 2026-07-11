import { listenTerminalExit, listenTerminalOutput } from "./tauri"

/**
 * Singleton fan-out for terminal output. The Tauri event listener must exist
 * BEFORE `terminal_create` resolves, otherwise the shell's first output (the
 * prompt) races the per-instance subscription and is lost — xterm instances
 * mount asynchronously after create, and StrictMode remounts widen the gap.
 * The bus subscribes once, buffers per-terminal scrollback, and replays it to
 * every late subscriber, so remounts and tab switches never drop output.
 */

type TerminalSubscriber = (data: string) => void

const MAX_BUFFER_CHARS = 200_000
const EXIT_NOTE = "\r\n\x1b[90m[process exited]\x1b[0m\r\n"

const buffers = new Map<string, string>()
const subscribers = new Map<string, Set<TerminalSubscriber>>()
let started = false
let startPromise: Promise<void> | null = null

const dispatch = (id: string, data: string) => {
  const prev = buffers.get(id) ?? ""
  buffers.set(id, (prev + data).slice(-MAX_BUFFER_CHARS))
  const subs = subscribers.get(id)
  if (subs) {
    for (const sub of subs) sub(data)
  }
}

/** Idempotent; await before any `terminalCreate` so no early output is lost. */
export const ensureTerminalBus = (): Promise<void> => {
  if (started) return Promise.resolve()
  if (startPromise) return startPromise
  startPromise = (async () => {
    // #region agent log
    fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Debug-Session-Id": "34bae6",
      },
      body: JSON.stringify({
        sessionId: "34bae6",
        runId: "pre-fix",
        hypothesisId: "H1",
        location: "terminalBus.ts:ensureTerminalBus",
        message: "starting listeners",
        data: {},
        timestamp: Date.now(),
      }),
    }).catch(() => {})
    // #endregion
    await listenTerminalOutput((e) => {
      // #region agent log
      fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-Debug-Session-Id": "34bae6",
        },
        body: JSON.stringify({
          sessionId: "34bae6",
          runId: "pre-fix",
          hypothesisId: "H1",
          location: "terminalBus.ts:onOutput",
          message: "frontend received terminal-output",
          data: { id: e.id, dataLen: e.data?.length ?? 0 },
          timestamp: Date.now(),
        }),
      }).catch(() => {})
      // #endregion
      dispatch(e.id, e.data)
    })
    await listenTerminalExit((e) => dispatch(e.id, EXIT_NOTE))
    started = true
    // #region agent log
    fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Debug-Session-Id": "34bae6",
      },
      body: JSON.stringify({
        sessionId: "34bae6",
        runId: "pre-fix",
        hypothesisId: "H1",
        location: "terminalBus.ts:ensureTerminalBus",
        message: "listeners ready",
        data: { started: true },
        timestamp: Date.now(),
      }),
    }).catch(() => {})
    // #endregion
  })().catch((err) => {
    startPromise = null
    throw err
  })
  return startPromise
}

/**
 * Subscribe an xterm instance to a terminal's output. Replays the buffered
 * scrollback synchronously before live chunks. Returns an unsubscribe fn.
 */
export const subscribeTerminal = (
  id: string,
  onData: TerminalSubscriber,
): (() => void) => {
  void ensureTerminalBus()
  const buffered = buffers.get(id)
  if (buffered) onData(buffered)
  let subs = subscribers.get(id)
  if (!subs) {
    subs = new Set()
    subscribers.set(id, subs)
  }
  subs.add(onData)
  return () => {
    subs?.delete(onData)
  }
}

export const dropTerminalBuffer = (id: string) => {
  buffers.delete(id)
  subscribers.delete(id)
}

/**
 * Push data into a terminal's buffer/fan-out from a source other than the
 * backend PTY (e.g. agent exec_chunk session-events routed to a synthetic
 * `agent:${sessionId}` terminal id). Buffers and dispatches exactly like PTY
 * output, so late-subscribing xterm instances still replay scrollback.
 */
export const pushTerminalData = (id: string, data: string): void => {
  dispatch(id, data)
}

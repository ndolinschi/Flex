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

const dispatch = (id: string, data: string) => {
  const prev = buffers.get(id) ?? ""
  buffers.set(id, (prev + data).slice(-MAX_BUFFER_CHARS))
  const subs = subscribers.get(id)
  if (subs) {
    for (const sub of subs) sub(data)
  }
}

/** Idempotent; call before any `terminalCreate` so no early output is lost. */
export const ensureTerminalBus = (): void => {
  if (started) return
  started = true
  void listenTerminalOutput((e) => dispatch(e.id, e.data))
  void listenTerminalExit((e) => dispatch(e.id, EXIT_NOTE))
}

/**
 * Subscribe an xterm instance to a terminal's output. Replays the buffered
 * scrollback synchronously before live chunks. Returns an unsubscribe fn.
 */
export const subscribeTerminal = (
  id: string,
  onData: TerminalSubscriber,
): (() => void) => {
  ensureTerminalBus()
  const buffered = buffers.get(id)
  if (buffered) onData(buffered)
  let subs = subscribers.get(id)
  if (!subs) {
    subs = new Set()
    subscribers.set(id, subs)
  }
  subs.add(onData)
  return () => {
    subs.delete(onData)
  }
}

/** Forget a killed terminal's scrollback. */
export const dropTerminalBuffer = (id: string): void => {
  buffers.delete(id)
  subscribers.delete(id)
}

import { listenTerminalExit, listenTerminalOutput } from "./tauri"

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

export const ensureTerminalBus = (): Promise<void> => {
  if (started) return Promise.resolve()
  if (startPromise) return startPromise
  startPromise = (async () => {
    await listenTerminalOutput((e) => {
      dispatch(e.id, e.data)
    })
    await listenTerminalExit((e) => dispatch(e.id, EXIT_NOTE))
    started = true
  })().catch((err) => {
    startPromise = null
    throw err
  })
  return startPromise
}

/**
 * Subscribe to terminal output for `id`. Replay the buffer immediately
 * (unbatched, so attach is complete), then batch subsequent chunks into a
 * single `onData(joined)` call per animation frame to cut xterm write cost.
 */
export const subscribeTerminal = (
  id: string,
  onData: TerminalSubscriber,
): (() => void) => {
  void ensureTerminalBus()
  const buffered = buffers.get(id)
  if (buffered) onData(buffered)

  let pending = ""
  let raf = 0
  const flush = () => {
    raf = 0
    if (!pending) return
    const chunk = pending
    pending = ""
    onData(chunk)
  }
  const batched: TerminalSubscriber = (data) => {
    pending += data
    if (!raf) {
      raf = requestAnimationFrame(flush)
    }
  }

  let subs = subscribers.get(id)
  if (!subs) {
    subs = new Set()
    subscribers.set(id, subs)
  }
  subs.add(batched)
  return () => {
    if (raf) {
      cancelAnimationFrame(raf)
      raf = 0
    }
    // Flush any pending bytes so subscribers don't lose a trailing chunk
    // when the component unsubscribes mid-frame.
    if (pending) {
      const chunk = pending
      pending = ""
      onData(chunk)
    }
    subs?.delete(batched)
  }
}

export const dropTerminalBuffer = (id: string) => {
  buffers.delete(id)
  subscribers.delete(id)
}

export const pushTerminalData = (id: string, data: string): void => {
  dispatch(id, data)
}

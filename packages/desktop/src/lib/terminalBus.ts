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

export const pushTerminalData = (id: string, data: string): void => {
  dispatch(id, data)
}

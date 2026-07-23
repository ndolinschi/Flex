
import { scanExecChunk } from "./execErrorScan"

type TailSubscriber = () => void

const MAX_TAIL_CHARS = 2_000

const MAX_TAIL_BUFFERS = 50

const tails = new Map<string, string>()
const subscribers = new Map<string, Set<TailSubscriber>>()

const notify = (callId: string) => {
  const subs = subscribers.get(callId)
  if (subs) {
    for (const sub of subs) sub()
  }
}

const evictOldestIfNeeded = () => {
  if (tails.size <= MAX_TAIL_BUFFERS) return
  for (const key of tails.keys()) {
    if (tails.size <= MAX_TAIL_BUFFERS) break
    if (subscribers.get(key)?.size) continue
    tails.delete(key)
  }
}

export const pushExecTail = (callId: string, text: string): void => {
  const prev = tails.get(callId) ?? ""
  tails.set(callId, (prev + text).slice(-MAX_TAIL_CHARS))
  evictOldestIfNeeded()
  scanExecChunk(callId, text)
  notify(callId)
}

export const getExecTail = (callId: string): string => tails.get(callId) ?? ""

export const clearExecTail = (callId: string): void => {
  tails.delete(callId)
  subscribers.delete(callId)
}

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

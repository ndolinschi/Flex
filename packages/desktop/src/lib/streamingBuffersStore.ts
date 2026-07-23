import { useSyncExternalStore } from "react"
import type { StreamingBuffers } from "./types"
import { emptyStreaming } from "../stores/types"

/**
 * Per-session live streaming buffers outside the global Zustand store.
 * High-frequency token deltas must not notify layout/sidebar subscribers.
 */

const buffersBySession = new Map<string, StreamingBuffers>()
const listenersBySession = new Map<string, Set<() => void>>()
const versionBySession = new Map<string, number>()

const EMPTY: StreamingBuffers = emptyStreaming()

const bump = (sessionId: string): void => {
  versionBySession.set(sessionId, (versionBySession.get(sessionId) ?? 0) + 1)
  const listeners = listenersBySession.get(sessionId)
  if (!listeners) return
  for (const listener of listeners) listener()
}

export const getStreamingBuffers = (sessionId: string): StreamingBuffers =>
  buffersBySession.get(sessionId) ?? EMPTY

export const getStreamingBuffersVersion = (sessionId: string): number =>
  versionBySession.get(sessionId) ?? 0

export const setStreamingBuffers = (
  sessionId: string,
  buffers: StreamingBuffers,
): void => {
  const prev = buffersBySession.get(sessionId)
  if (prev === buffers) return
  buffersBySession.set(sessionId, buffers)
  bump(sessionId)
}

export const updateStreamingBuffers = (
  sessionId: string,
  updater: (prev: StreamingBuffers) => StreamingBuffers,
): StreamingBuffers => {
  const prev = buffersBySession.get(sessionId) ?? emptyStreaming()
  const next = updater(prev)
  if (next === prev) return prev
  buffersBySession.set(sessionId, next)
  bump(sessionId)
  return next
}

export const clearStreamingBuffers = (sessionId: string): void => {
  const empty = emptyStreaming()
  const prev = buffersBySession.get(sessionId)
  if (
    prev &&
    Object.keys(prev.markdown).length === 0 &&
    Object.keys(prev.thinking).length === 0 &&
    Object.keys(prev.toolCalls).length === 0 &&
    Object.keys(prev.toolProgress).length === 0 &&
    Object.keys(prev.toolArgs).length === 0
  ) {
    return
  }
  buffersBySession.set(sessionId, empty)
  bump(sessionId)
}

export const dropStreamingBuffers = (sessionId: string): void => {
  if (!buffersBySession.has(sessionId)) return
  buffersBySession.delete(sessionId)
  bump(sessionId)
}

export const subscribeStreamingBuffers = (
  sessionId: string,
  onStoreChange: () => void,
): (() => void) => {
  let set = listenersBySession.get(sessionId)
  if (!set) {
    set = new Set()
    listenersBySession.set(sessionId, set)
  }
  set.add(onStoreChange)
  return () => {
    set?.delete(onStoreChange)
    if (set && set.size === 0) listenersBySession.delete(sessionId)
  }
}

/** React subscription scoped to one session's streaming buffers. */
export const useStreamingBuffers = (
  sessionId: string | null,
): StreamingBuffers => {
  return useSyncExternalStore(
    (onStoreChange) => {
      if (!sessionId) return () => {}
      return subscribeStreamingBuffers(sessionId, onStoreChange)
    },
    () => (sessionId ? getStreamingBuffers(sessionId) : EMPTY),
    () => EMPTY,
  )
}

/** Test helper — clear all buffers between cases. */
export const __resetStreamingBuffersStoreForTests = (): void => {
  buffersBySession.clear()
  listenersBySession.clear()
  versionBySession.clear()
}

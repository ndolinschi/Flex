import { useCallback, useEffect, useRef, useState } from "react"
import { withProgrammaticScroll } from "../lib/programmaticScroll"

/** Stick-to-bottom scroll behavior for the chat timeline pane.
 * `streamContentKey` is a narrow primitive (e.g. summed streaming text
 * lengths) — do not pass the whole streaming object.
 *
 * While streaming, the virtualizer's `followOnAppend` / `anchorTo: "end"`
 * already grows the viewport; we only re-stick on row-count / layout
 * changes so we don't fight remounts on every token (jumpy feed) or fire
 * global scroll listeners that dismiss ContextMenus. */
export const useStickToBottom = (
  liveRowsLength: number,
  isStreaming: boolean,
  streamContentKey: number,
) => {
  const scrollRef = useRef<HTMLDivElement>(null)
  const bottomRef = useRef<HTMLDivElement>(null)
  const stickToBottomRef = useRef(true)
  const scrollRafRef = useRef<number | null>(null)
  const [showScrollDown, setShowScrollDown] = useState(false)

  const scrollToBottom = useCallback((smooth = false) => {
    const el = scrollRef.current
    if (!el) return
    // Don't yank scroll while the user is selecting text in the timeline.
    const sel = window.getSelection()
    if (sel && !sel.isCollapsed && el.contains(sel.anchorNode)) return

    withProgrammaticScroll(() => {
      if (smooth) {
        el.scrollTo({ top: el.scrollHeight, behavior: "smooth" })
      } else {
        // Already glued to the bottom — skip so we don't thrash scroll
        // listeners / virtualizer anchors on every tiny growth.
        const distance = el.scrollHeight - el.scrollTop - el.clientHeight
        if (!smooth && distance < 2) return
        el.scrollTop = el.scrollHeight
      }
    })
  }, [])

  const scheduleScrollToBottom = useCallback(() => {
    if (!stickToBottomRef.current) return
    if (scrollRafRef.current !== null) return
    scrollRafRef.current = window.requestAnimationFrame(() => {
      scrollRafRef.current = null
      scrollToBottom(false)
    })
  }, [scrollToBottom])

  const handleScroll = useCallback(() => {
    const el = scrollRef.current
    if (!el) return
    const distance = el.scrollHeight - el.scrollTop - el.clientHeight
    const nearBottom = distance < 80
    stickToBottomRef.current = nearBottom
    setShowScrollDown(!nearBottom && liveRowsLength > 0)
  }, [liveRowsLength])

  const handleScrollToBottom = useCallback(() => {
    stickToBottomRef.current = true
    setShowScrollDown(false)
    scrollToBottom(true)
  }, [scrollToBottom])

  /** Re-stick after work groups expand/collapse (content height changes). */
  const handleLayoutChange = useCallback(() => {
    scheduleScrollToBottom()
  }, [scheduleScrollToBottom])

  // New rows / streaming start-stop: re-stick. Per-token streamContentKey is
  // intentionally omitted while streaming — virtualizer followOnAppend owns
  // that path; forcing scrollTop every delta caused jumps + menu dismissals.
  useEffect(() => {
    if (liveRowsLength === 0) return
    scheduleScrollToBottom()
  }, [liveRowsLength, isStreaming, scheduleScrollToBottom])

  // Settled (non-streaming) content edits that don't change row count still
  // need a gentle re-stick when the user is pinned (e.g. markdown settle).
  useEffect(() => {
    if (isStreaming || liveRowsLength === 0) return
    scheduleScrollToBottom()
  }, [streamContentKey, isStreaming, liveRowsLength, scheduleScrollToBottom])

  useEffect(() => {
    return () => {
      if (scrollRafRef.current !== null) {
        cancelAnimationFrame(scrollRafRef.current)
      }
    }
  }, [])

  return {
    scrollRef,
    bottomRef,
    showScrollDown,
    handleScroll,
    handleScrollToBottom,
    handleLayoutChange,
  }
}

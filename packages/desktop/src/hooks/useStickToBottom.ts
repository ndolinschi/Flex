import { useCallback, useEffect, useRef, useState } from "react"

/** Stick-to-bottom scroll behavior for the chat timeline pane.
 * `streamContentKey` is a narrow primitive (e.g. summed streaming text
 * lengths) — do not pass the whole streaming object. */
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
    if (smooth) {
      el.scrollTo({ top: el.scrollHeight, behavior: "smooth" })
    } else {
      el.scrollTop = el.scrollHeight
    }
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

  useEffect(() => {
    if (liveRowsLength === 0) return
    scheduleScrollToBottom()
  }, [liveRowsLength, isStreaming, streamContentKey, scheduleScrollToBottom])

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

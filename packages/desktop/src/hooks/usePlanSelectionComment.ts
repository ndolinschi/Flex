import { useCallback, useEffect, useState, type RefObject } from "react"

export type PlanSelectionAnchor = {
  quote: string
  startOffset: number
  endOffset: number
  anchor: { x: number; y: number }
}

const offsetWithin = (
  root: HTMLElement,
  targetNode: Node,
  targetOffset: number,
): number | null => {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
  let total = 0
  let n = walker.nextNode()
  while (n) {
    if (n === targetNode) return total + targetOffset
    total += (n.nodeValue ?? "").length
    n = walker.nextNode()
  }
  return null
}

export const usePlanSelectionComment = (
  containerRef: RefObject<HTMLElement | null>,
  enabled: boolean,
) => {
  const [selection, setSelection] = useState<PlanSelectionAnchor | null>(null)
  const [composerOpen, setComposerOpen] = useState(false)

  const clearSelection = useCallback(() => {
    setSelection(null)
    setComposerOpen(false)
  }, [])

  const openComposer = useCallback(() => {
    if (selection) setComposerOpen(true)
  }, [selection])

  useEffect(() => {
    if (!enabled) {
      setSelection(null)
      setComposerOpen(false)
      return
    }

    const onMouseUp = () => {
      if (composerOpen) return

      const root = containerRef.current
      if (!root) return
      const sel = window.getSelection()
      if (!sel || sel.isCollapsed || sel.rangeCount === 0) {
        setSelection(null)
        return
      }

      const range = sel.getRangeAt(0)
      if (!root.contains(range.commonAncestorContainer)) {
        setSelection(null)
        return
      }

      const quote = sel.toString().replace(/\s+/g, " ").trim()
      if (!quote) {
        setSelection(null)
        return
      }

      const startOffset = offsetWithin(
        root,
        range.startContainer,
        range.startOffset,
      )
      const endOffset = offsetWithin(root, range.endContainer, range.endOffset)
      if (startOffset === null || endOffset === null || endOffset <= startOffset) {
        setSelection(null)
        return
      }

      const rect = range.getBoundingClientRect()
      setSelection({
        quote,
        startOffset,
        endOffset,
        anchor: {
          x: rect.left + rect.width / 2,
          y: rect.bottom + 8,
        },
      })
    }

    document.addEventListener("mouseup", onMouseUp)
    return () => document.removeEventListener("mouseup", onMouseUp)
  }, [containerRef, enabled, composerOpen])

  return {
    selection,
    composerOpen,
    openComposer,
    clearSelection,
    draft: composerOpen && selection ? selection : null,
  }
}

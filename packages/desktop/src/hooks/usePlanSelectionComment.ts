import { useCallback, useEffect, useState, type RefObject } from "react"
import type { PlanCommentDraft } from "../components/molecules/PlanCommentPopover"

/** Compute the character offset of `node`/`offset` within `root`'s text. */
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

/**
 * Capture a non-empty text selection inside the plan body and expose a
 * draft suitable for `PlanCommentPopover`.
 */
export const usePlanSelectionComment = (
  containerRef: RefObject<HTMLElement | null>,
  enabled: boolean,
) => {
  const [draft, setDraft] = useState<PlanCommentDraft | null>(null)

  const clearDraft = useCallback(() => setDraft(null), [])

  useEffect(() => {
    if (!enabled) {
      setDraft(null)
      return
    }

    const onMouseUp = () => {
      const root = containerRef.current
      if (!root) return
      const sel = window.getSelection()
      if (!sel || sel.isCollapsed || sel.rangeCount === 0) return

      const range = sel.getRangeAt(0)
      if (!root.contains(range.commonAncestorContainer)) return

      const quote = sel.toString().replace(/\s+/g, " ").trim()
      if (!quote) return

      const startOffset = offsetWithin(
        root,
        range.startContainer,
        range.startOffset,
      )
      const endOffset = offsetWithin(root, range.endContainer, range.endOffset)
      if (startOffset === null || endOffset === null || endOffset <= startOffset) {
        return
      }

      const rect = range.getBoundingClientRect()
      setDraft({
        quote,
        startOffset,
        endOffset,
        anchor: {
          x: rect.left,
          y: rect.bottom + 8,
        },
      })
    }

    document.addEventListener("mouseup", onMouseUp)
    return () => document.removeEventListener("mouseup", onMouseUp)
  }, [containerRef, enabled])

  return { draft, setDraft, clearDraft }
}

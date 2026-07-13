import { useEffect, type RefObject } from "react"
import type { PlanComment } from "../stores/types"

const MARK_ATTR = "data-plan-comment-id"
const MARK_SELECTOR = `mark[${MARK_ATTR}]`
const ACTIVE_CLASS = "plan-comment-mark-active"

const clearCommentMarks = (root: HTMLElement) => {
  const marks = root.querySelectorAll<HTMLElement>(MARK_SELECTOR)
  for (const mark of marks) {
    const parent = mark.parentNode
    if (!parent) continue
    parent.replaceChild(document.createTextNode(mark.textContent ?? ""), mark)
    parent.normalize()
  }
}

const escapeRegExp = (s: string): string =>
  s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")

/**
 * Paint persistent comment highlights over the rendered plan markdown by
 * matching each comment's quote in the DOM text (same approach as
 * `usePlanFind`). Skips painting while Find-in-Plan is open.
 */
export const usePlanCommentHighlights = (
  containerRef: RefObject<HTMLElement | null>,
  comments: PlanComment[],
  activeCommentId: string | null,
  findOpen: boolean,
) => {
  const commentsKey = comments.map((c) => `${c.id}:${c.quote}`).join("|")

  useEffect(() => {
    const root = containerRef.current
    if (!root) return

    clearCommentMarks(root)
    if (findOpen || comments.length === 0) return

    const paint = () => {
      const el = containerRef.current
      if (!el) return
      clearCommentMarks(el)

      for (const comment of comments) {
        const quote = comment.quote.trim()
        if (!quote) continue
        const re = new RegExp(escapeRegExp(quote), "i")

        const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT)
        const textNodes: Text[] = []
        let n = walker.nextNode()
        while (n) {
          textNodes.push(n as Text)
          n = walker.nextNode()
        }

        for (const textNode of textNodes) {
          const value = textNode.nodeValue ?? ""
          re.lastIndex = 0
          const m = re.exec(value)
          if (!m) continue

          const start = m.index ?? 0
          const end = start + m[0].length
          const frag = document.createDocumentFragment()
          if (start > 0) {
            frag.appendChild(document.createTextNode(value.slice(0, start)))
          }
          const mark = document.createElement("mark")
          mark.setAttribute(MARK_ATTR, comment.id)
          mark.textContent = value.slice(start, end)
          frag.appendChild(mark)
          if (end < value.length) {
            frag.appendChild(document.createTextNode(value.slice(end)))
          }
          textNode.parentNode?.replaceChild(frag, textNode)
          break // one highlight per comment
        }
      }
    }

    const raf = requestAnimationFrame(paint)
    return () => {
      cancelAnimationFrame(raf)
      const el = containerRef.current
      if (el) clearCommentMarks(el)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [containerRef, commentsKey, findOpen])

  useEffect(() => {
    const root = containerRef.current
    if (!root || findOpen) return
    const marks = root.querySelectorAll<HTMLElement>(MARK_SELECTOR)
    for (const mark of marks) {
      const id = mark.getAttribute(MARK_ATTR)
      if (id && id === activeCommentId) {
        mark.classList.add(ACTIVE_CLASS)
        mark.scrollIntoView({ block: "center", behavior: "smooth" })
      } else {
        mark.classList.remove(ACTIVE_CLASS)
      }
    }
  }, [containerRef, activeCommentId, commentsKey, findOpen])
}

export const scrollToPlanComment = (
  container: HTMLElement | null,
  commentId: string,
) => {
  if (!container) return
  const mark = container.querySelector<HTMLElement>(
    `mark[${MARK_ATTR}="${CSS.escape(commentId)}"]`,
  )
  mark?.scrollIntoView({ block: "center", behavior: "smooth" })
  mark?.classList.add(ACTIVE_CLASS)
}

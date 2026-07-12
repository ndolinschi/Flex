import { useEffect, useRef, useState, type RefObject } from "react"

const MARK_ATTR = "data-plan-find-mark"
const MARK_SELECTOR = `mark[${MARK_ATTR}]`
const ACTIVE_CLASS = "plan-find-mark-active"

/** Escapes a string for use inside a `RegExp` (no special-char injection
 * from arbitrary Find-in-Plan queries). */
const escapeRegExp = (s: string): string => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")

/** Removes any highlight `<mark>`s inserted by a previous pass, merging
 * their text back into the surrounding text node (so re-running the
 * search on updated content starts from clean DOM). */
const clearMarks = (root: HTMLElement) => {
  const marks = root.querySelectorAll<HTMLElement>(MARK_SELECTOR)
  for (const mark of marks) {
    const parent = mark.parentNode
    if (!parent) continue
    const text = document.createTextNode(mark.textContent ?? "")
    parent.replaceChild(text, mark)
    parent.normalize()
  }
}

/**
 * Find-in-Plan: wraps every case-insensitive match of `query` in the
 * rendered plan markdown with a `<mark>`, without touching react-markdown's
 * own rendering — this walks the DOM text nodes AFTER MarkdownBody paints,
 * which is the simplest robust option since matches can straddle inline
 * markdown formatting in ways a source-text regex can't safely see.
 *
 * Returns the match count and active index, and exposes `scrollToActive` /
 * `next` / `prev` to drive the toolbar's counter + navigation.
 */
export const usePlanFind = (
  containerRef: RefObject<HTMLElement | null>,
  query: string,
  active: boolean,
) => {
  const [matchCount, setMatchCount] = useState(0)
  const [activeIndex, setActiveIndex] = useState(0)
  const activeIndexRef = useRef(0)

  useEffect(() => {
    const root = containerRef.current
    if (!root) return

    if (!active || !query.trim()) {
      clearMarks(root)
      setMatchCount(0)
      setActiveIndex(0)
      return
    }

    clearMarks(root)

    const re = new RegExp(escapeRegExp(query.trim()), "gi")
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode: (node) => {
        // Skip text inside <script>/<style> (none expected, defensive) and
        // marks we just cleared but might still be mid-walk over.
        const parentTag = node.parentElement?.tagName
        if (parentTag === "SCRIPT" || parentTag === "STYLE") {
          return NodeFilter.FILTER_REJECT
        }
        return NodeFilter.FILTER_ACCEPT
      },
    })

    const textNodes: Text[] = []
    let n = walker.nextNode()
    while (n) {
      textNodes.push(n as Text)
      n = walker.nextNode()
    }

    let total = 0
    for (const textNode of textNodes) {
      const value = textNode.nodeValue ?? ""
      re.lastIndex = 0
      const matches = [...value.matchAll(re)]
      if (matches.length === 0) continue

      const frag = document.createDocumentFragment()
      let cursor = 0
      for (const m of matches) {
        const start = m.index ?? 0
        const end = start + m[0].length
        if (start > cursor) {
          frag.appendChild(document.createTextNode(value.slice(cursor, start)))
        }
        const mark = document.createElement("mark")
        mark.setAttribute(MARK_ATTR, String(total))
        mark.textContent = value.slice(start, end)
        frag.appendChild(mark)
        cursor = end
        total += 1
      }
      if (cursor < value.length) {
        frag.appendChild(document.createTextNode(value.slice(cursor)))
      }
      textNode.parentNode?.replaceChild(frag, textNode)
    }

    setMatchCount(total)
    setActiveIndex((prev) => (total === 0 ? 0 : Math.min(prev, total - 1)))
  }, [containerRef, query, active])

  // Active-match styling + scroll-into-view, kept separate from the
  // (re)highlight pass so moving next/prev doesn't re-walk the DOM.
  useEffect(() => {
    const root = containerRef.current
    if (!root) return
    activeIndexRef.current = activeIndex
    const marks = root.querySelectorAll<HTMLElement>(MARK_SELECTOR)
    for (const mark of marks) {
      const idx = Number(mark.getAttribute(MARK_ATTR))
      if (idx === activeIndex) {
        mark.classList.add(ACTIVE_CLASS)
        mark.scrollIntoView({ block: "center", behavior: "smooth" })
      } else {
        mark.classList.remove(ACTIVE_CLASS)
      }
    }
  }, [containerRef, activeIndex, matchCount])

  // Cleanup marks on unmount / when Find closes.
  useEffect(() => {
    return () => {
      const root = containerRef.current
      if (root) clearMarks(root)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const next = () => {
    if (matchCount === 0) return
    setActiveIndex((i) => (i + 1) % matchCount)
  }
  const prev = () => {
    if (matchCount === 0) return
    setActiveIndex((i) => (i - 1 + matchCount) % matchCount)
  }

  return { matchCount, activeIndex, next, prev }
}

import { useEffect, useRef, useState, type RefObject } from "react"

const MARK_ATTR = "data-plan-find-mark"
const MARK_SELECTOR = `mark[${MARK_ATTR}]`
const ACTIVE_CLASS = "plan-find-mark-active"

const escapeRegExp = (s: string): string => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")

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

  useEffect(() => {
    return () => {
      const root = containerRef.current
      if (root) clearMarks(root)
    }
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

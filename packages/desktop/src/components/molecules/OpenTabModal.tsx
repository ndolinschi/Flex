import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import { createPortal } from "react-dom"
import { MessageSquare } from "lucide-react"
import type { LucideIcon } from "lucide-react"
import { CommandPaletteRow } from "./CommandPaletteRow"
import { fuzzyScore } from "../../lib/fuzzySearch"
import type { SessionId } from "../../lib/types"
import { cn } from "../../lib/utils"
import type { RightPanelTab } from "../../stores/appStore"

type OpenTabDef = {
  id: RightPanelTab
  label: string
  icon: LucideIcon
}

type OpenTabEntry = {
  id: string
  label: string
  icon: LucideIcon
  kind: "chat" | "tool"
  tool?: RightPanelTab
}

type OpenTabModalProps = {
  open: boolean
  onClose: () => void
  /** Viewport rect of the `+` button (or click point) — menu anchors below it. */
  anchor: { x: number; y: number; width?: number; height?: number } | null
  paneIndex: 0 | 1
  sessionId: SessionId | null
  /** Visible tool tabs for this session/workspace. */
  tabs: OpenTabDef[]
  onOpenChat: (paneIndex: 0 | 1, sessionId: SessionId) => void
  onOpenTool: (
    paneIndex: 0 | 1,
    sessionId: SessionId,
    tool: RightPanelTab,
  ) => void
}

const MENU_WIDTH = 280
const MARGIN = 8
/** Row height ≈ CommandPaletteRow py-1.5 + icon line (~32px). Show ~5, rest scroll. */
const ROW_HEIGHT_PX = 32
const VISIBLE_ROWS = 5
const LIST_MAX_HEIGHT_PX = ROW_HEIGHT_PX * VISIBLE_ROWS

/** Prefer everyday workspace tabs first; everything else follows catalog order. */
const PRIMARY_TOOL_ORDER: readonly RightPanelTab[] = [
  "plan",
  "changes",
  "files",
  "terminal",
  "browser",
] as const

const primaryRank = (id: RightPanelTab): number => {
  const i = PRIMARY_TOOL_ORDER.indexOf(id)
  return i === -1 ? PRIMARY_TOOL_ORDER.length + 1 : i
}

const buildEntries = (
  catalog: OpenTabDef[],
  hasSession: boolean,
): OpenTabEntry[] => {
  const out: OpenTabEntry[] = []
  if (hasSession) {
    out.push({
      id: "chat",
      label: "Chat",
      icon: MessageSquare,
      kind: "chat",
    })
  }
  const sorted = [...catalog].sort((a, b) => {
    const d = primaryRank(a.id) - primaryRank(b.id)
    if (d !== 0) return d
    return a.label.localeCompare(b.label)
  })
  for (const t of sorted) {
    out.push({
      id: `tool:${t.id}`,
      label: t.label,
      icon: t.icon,
      kind: "tool",
      tool: t.id,
    })
  }
  return out
}

/** Searchable open-tab picker for ContentPane `+` (catalog-driven).
 * Idle list shows ~5 primary tabs; the rest stay a short scroll away. */
export const OpenTabModal = ({
  open,
  onClose,
  anchor,
  paneIndex,
  sessionId,
  tabs,
  onOpenChat,
  onOpenTool,
}: OpenTabModalProps) => {
  const [query, setQuery] = useState("")
  const [activeIndex, setActiveIndex] = useState(0)
  const [coords, setCoords] = useState<{ top: number; left: number } | null>(
    null,
  )
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const panelRef = useRef<HTMLDivElement>(null)
  const onCloseRef = useRef(onClose)
  onCloseRef.current = onClose

  const entries = useMemo(
    () => buildEntries(tabs, !!sessionId),
    [tabs, sessionId],
  )

  const filtered = useMemo(() => {
    const q = query.trim()
    if (!q) return entries
    return entries
      .map((e) => ({ e, score: fuzzyScore(q, e.label) }))
      .filter((r): r is { e: OpenTabEntry; score: number } => r.score !== null)
      .sort((a, b) => a.score - b.score)
      .map((r) => r.e)
  }, [entries, query])

  const activate = (entry: OpenTabEntry) => {
    if (!sessionId) return
    if (entry.kind === "chat") {
      onOpenChat(paneIndex, sessionId)
    } else if (entry.tool) {
      onOpenTool(paneIndex, sessionId, entry.tool)
    }
    onClose()
  }

  useEffect(() => {
    setActiveIndex(0)
  }, [query, open])

  useEffect(() => {
    if (open) setQuery("")
  }, [open])

  useEffect(() => {
    if (!open) return
    const el = inputRef.current
    if (el) requestAnimationFrame(() => el.focus())
  }, [open])

  // Anchor below the `+` button; flip/clamp to stay in the viewport.
  useLayoutEffect(() => {
    if (!open || !anchor) {
      setCoords(null)
      return
    }
    const el = panelRef.current
    const h = el?.offsetHeight ?? 220
    const w = el?.offsetWidth ?? MENU_WIDTH
    const aw = anchor.width ?? 0
    const ah = anchor.height ?? 0
    let left = anchor.x
    let top = anchor.y + ah + 4
    if (left + w + MARGIN > window.innerWidth) {
      left = Math.max(MARGIN, anchor.x + aw - w)
    }
    if (top + h + MARGIN > window.innerHeight) {
      top = Math.max(MARGIN, anchor.y - h - 4)
    }
    left = Math.max(MARGIN, Math.min(left, window.innerWidth - w - MARGIN))
    setCoords({ top, left })
  }, [open, anchor, filtered.length])

  useEffect(() => {
    if (!open) return

    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault()
        onCloseRef.current()
        return
      }
      if (e.key === "ArrowDown") {
        e.preventDefault()
        setActiveIndex((i) => Math.min(i + 1, filtered.length - 1))
        return
      }
      if (e.key === "ArrowUp") {
        e.preventDefault()
        setActiveIndex((i) => Math.max(i - 1, 0))
        return
      }
      if (e.key === "Enter") {
        e.preventDefault()
        const entry = filtered[activeIndex]
        if (entry) activate(entry)
      }
    }

    const handlePointerDown = (e: PointerEvent) => {
      const target = e.target as Node
      if (panelRef.current?.contains(target)) return
      onCloseRef.current()
    }

    window.addEventListener("keydown", handleKey)
    window.addEventListener("pointerdown", handlePointerDown, true)
    return () => {
      window.removeEventListener("keydown", handleKey)
      window.removeEventListener("pointerdown", handlePointerDown, true)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, filtered, activeIndex, sessionId, paneIndex])

  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>(
      `[data-index="${activeIndex}"]`,
    )
    el?.scrollIntoView({ block: "nearest" })
  }, [activeIndex])

  if (!open || !anchor) return null

  return createPortal(
    <div
      ref={panelRef}
      className={cn(
        "fixed z-[300] flex w-[280px] flex-col overflow-hidden",
        "rounded-lg bg-panel shadow-[var(--shadow-popover)] animate-tray-in",
        !coords && "invisible",
      )}
      style={
        coords
          ? { top: coords.top, left: coords.left }
          : { top: anchor.y + (anchor.height ?? 0) + 4, left: anchor.x }
      }
      role="dialog"
      aria-modal="true"
      aria-label="Open tab"
    >
      <div className="flex shrink-0 items-center gap-1.5 border-b border-stroke-3 px-2.5 py-2">
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Open a tab…"
          aria-label="Open tab search"
          className="w-full bg-transparent text-sm text-ink outline-none placeholder:text-ink-faint"
        />
      </div>
      <div
        ref={listRef}
        className="overflow-y-auto py-1"
        style={{ maxHeight: LIST_MAX_HEIGHT_PX }}
        role="listbox"
        aria-label="Tabs"
      >
        {filtered.length === 0 ? (
          <p className="px-2.5 py-2 text-sm text-ink-muted">No matching tabs</p>
        ) : (
          filtered.map((entry, i) => (
            <CommandPaletteRow
              key={entry.id}
              index={i}
              active={i === activeIndex}
              label={entry.label}
              icon={entry.icon}
              onActivate={() => activate(entry)}
              onHover={() => setActiveIndex(i)}
            />
          ))
        )}
      </div>
    </div>,
    document.body,
  )
}

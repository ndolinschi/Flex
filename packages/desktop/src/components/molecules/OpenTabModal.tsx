import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import { createPortal } from "react-dom"
import { MessageSquare } from "@/components/icons"
import type { Icon } from "@/components/icons"
import { fuzzyScore } from "../../lib/fuzzySearch"
import type { SessionId } from "../../lib/types"
import { cn } from "../../lib/utils"
import type { RightPanelTab } from "../../stores/appStore"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"

type OpenTabDef = {
  id: RightPanelTab
  label: string
  icon: Icon
}

type OpenTabEntry = {
  id: string
  label: string
  icon: Icon
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
  for (const t of catalog) {
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

/** Searchable open-tab picker for ContentPane `+` (catalog-driven). */
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
  const [coords, setCoords] = useState<{ top: number; left: number } | null>(
    null,
  )
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
    if (open) setQuery("")
  }, [open])

  // Anchor below the `+` button; flip/clamp to stay in the viewport.
  useLayoutEffect(() => {
    if (!open || !anchor) {
      setCoords(null)
      return
    }
    const el = panelRef.current
    const h = el?.offsetHeight ?? 320
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
    const handlePointerDown = (e: PointerEvent) => {
      const target = e.target as Node
      if (panelRef.current?.contains(target)) return
      onCloseRef.current()
    }
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault()
        onCloseRef.current()
      }
    }
    window.addEventListener("pointerdown", handlePointerDown, true)
    window.addEventListener("keydown", handleKey)
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, true)
      window.removeEventListener("keydown", handleKey)
    }
  }, [open])

  if (!open || !anchor) return null

  return createPortal(
    <div
      ref={panelRef}
      className={cn(
        "fixed z-[300] flex max-h-[min(50vh,360px)] w-[280px] flex-col overflow-hidden",
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
      data-suppress-native-webview=""
    >
      <Command
        shouldFilter={false}
        className="max-h-[min(50vh,360px)] rounded-lg bg-panel"
      >
        <CommandInput
          value={query}
          onValueChange={setQuery}
          placeholder="Open a tab…"
          aria-label="Open tab search"
          className="text-sm"
        />
        <CommandList className="max-h-[min(40vh,300px)] py-1">
          <CommandEmpty>No matching tabs</CommandEmpty>
          <CommandGroup>
            {filtered.map((entry) => {
              const ItemIcon = entry.icon
              return (
                <CommandItem
                  key={entry.id}
                  value={entry.id}
                  onSelect={() => activate(entry)}
                >
                  <ItemIcon className="size-3.5 text-ink-muted" aria-hidden />
                  <span className="min-w-0 flex-1 truncate">{entry.label}</span>
                </CommandItem>
              )
            })}
          </CommandGroup>
        </CommandList>
      </Command>
    </div>,
    document.body,
  )
}

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import { createPortal } from "react-dom"
import { MessageSquare } from "lucide-react"
import type { LucideIcon } from "lucide-react"
import { CommandInput as CmdkInput } from "cmdk"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
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
 * Anchored near the trigger; ~5 primary tabs visible, remainder scrolls. */
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
  }, [open])

  if (!open || !anchor) return null

  const showGroups = !query.trim()
  const chatFiltered = filtered.filter((e) => e.kind === "chat")
  const toolFiltered = filtered.filter((e) => e.kind === "tool")

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
      aria-label="Open tab"
    >
      <Command
        shouldFilter={false}
        className="rounded-none bg-transparent p-0"
      >
        <div className="flex shrink-0 items-center gap-1.5 border-b border-stroke-3 px-2.5 py-2">
          <CmdkInput
            value={query}
            onValueChange={setQuery}
            placeholder="Open a tab…"
            autoFocus
            className="w-full bg-transparent text-sm text-ink outline-none placeholder:text-ink-faint"
          />
        </div>
        <CommandList
          className="py-1"
          style={{ maxHeight: 160 }}
        >
          <CommandEmpty className="px-2.5 py-2 text-sm text-ink-muted">
            No matching tabs
          </CommandEmpty>
          {chatFiltered.length > 0 && (
            <CommandGroup heading={showGroups ? "Chat" : undefined}>
              {chatFiltered.map((entry) => {
                const Icon = entry.icon
                return (
                  <CommandItem
                    key={entry.id}
                    value={entry.id}
                    onSelect={() => activate(entry)}
                    className="px-2.5"
                  >
                    <Icon aria-hidden />
                    <span className="min-w-0 truncate">{entry.label}</span>
                  </CommandItem>
                )
              })}
            </CommandGroup>
          )}
          {toolFiltered.length > 0 && (
            <CommandGroup heading={showGroups ? "Tools" : undefined}>
              {toolFiltered.map((entry) => {
                const Icon = entry.icon
                return (
                  <CommandItem
                    key={entry.id}
                    value={entry.id}
                    onSelect={() => activate(entry)}
                    className="px-2.5"
                  >
                    <Icon aria-hidden />
                    <span className="min-w-0 truncate">{entry.label}</span>
                  </CommandItem>
                )
              })}
            </CommandGroup>
          )}
        </CommandList>
      </Command>
    </div>,
    document.body,
  )
}

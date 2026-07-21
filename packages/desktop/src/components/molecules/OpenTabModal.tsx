import { useEffect, useMemo, useState } from "react"
import { MessageSquare } from "lucide-react"
import type { LucideIcon } from "lucide-react"
import { Input } from "@/components/ui/input"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from "@/components/ui/popover"
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

  const isOpen = open && !!anchor
  const showGroups = !query.trim()
  const chatFiltered = filtered.filter((e) => e.kind === "chat")
  const toolFiltered = filtered.filter((e) => e.kind === "tool")

  return (
    <Popover
      open={isOpen}
      onOpenChange={(next) => {
        if (!next) onClose()
      }}
    >
      {anchor ? (
        <PopoverTrigger
          nativeButton={false}
          tabIndex={-1}
          render={
            <span
              aria-hidden
              className="pointer-events-none fixed"
              style={{
                left: anchor.x,
                top: anchor.y,
                width: Math.max(anchor.width ?? 0, 1),
                height: Math.max(anchor.height ?? 0, 1),
              }}
            />
          }
        />
      ) : null}
      <PopoverContent
        side="bottom"
        align="start"
        sideOffset={4}
        positionerClassName="z-[300]"
        finalFocus={false}
        className="w-[280px] gap-0 overflow-hidden p-0"
      >
        <PopoverTitle className="sr-only">Open tab</PopoverTitle>
        <Command
          shouldFilter={false}
          className="rounded-none bg-transparent p-0"
        >
          <div className="flex shrink-0 items-center gap-1.5 border-b border-stroke-3 px-2.5 py-1.5">
            {/* Bare field: CommandInput wraps InputGroup with inset fill. Keep the
             * header one surface; aria-label names the field for AT. */}
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Open a tab…"
              aria-label="Open a tab"
              autoFocus
              className={cn(
                "h-auto min-w-0 flex-1 border-0 bg-transparent px-0 py-0 text-sm text-ink shadow-none",
                "rounded-none placeholder:text-ink-faint",
                "focus-visible:border-transparent focus-visible:ring-0",
                "dark:bg-transparent dark:disabled:bg-transparent",
              )}
            />
          </div>
          <CommandList className="py-1" style={{ maxHeight: 160 }}>
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
      </PopoverContent>
    </Popover>
  )
}

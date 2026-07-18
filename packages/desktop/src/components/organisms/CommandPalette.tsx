import { useEffect, useMemo, useRef, useState } from "react"
import { createPortal } from "react-dom"
import {
  Bot,
  Brain,
  Bug,
  Moon,
  Network,
  PanelLeft,
  PanelRight,
  SlidersHorizontal,
  Sparkles,
  SquarePen,
  Settings as SettingsIcon,
  MessagesSquare,
} from "lucide-react"
import type { LucideIcon } from "lucide-react"
import { CommandPaletteRow } from "../molecules"
import { useSessions } from "../../hooks/useSessions"
import {
  AUTOMATIONS_UI_ENABLED,
  FLEX_MODE_ENABLED,
} from "../../lib/featureFlags"
import { fuzzyScore } from "../../lib/fuzzySearch"
import { resumeSession, toInvokeError } from "../../lib/tauri"
import { sessionLabel } from "../../lib/types"
import { basename, cn } from "../../lib/utils"
import { useAppStore, type RightPanelTab } from "../../stores/appStore"
import { visibleRightPanelTabs } from "./right-panel/tabs"
import { log } from "../../lib/debug/log"

type CommandEntry = {
  id: string
  label: string
  icon?: LucideIcon
  group: "Commands" | "Sessions"
  hint?: string
  run: () => void
}

type CommandPaletteProps = {
  open: boolean
  onClose: () => void
}

/** Centered top overlay (the reference design/VS Code-style) — fuzzy command + session switcher. */
export const CommandPalette = ({ open, onClose }: CommandPaletteProps) => {
  const [query, setQuery] = useState("")
  const [activeIndex, setActiveIndex] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)

  const { sessions, newAgent } = useSessions()
  const setRoute = useAppStore((s) => s.setRoute)
  const toggleSidebarCollapsed = useAppStore((s) => s.toggleSidebarCollapsed)
  const toggleSplit = useAppStore((s) => s.toggleSplit)
  const openToolBesideChat = useAppStore((s) => s.openToolBesideChat)
  const toggleTheme = useAppStore((s) => s.toggleTheme)
  const setComposerMode = useAppStore((s) => s.setComposerMode)
  const setActiveSessionId = useAppStore((s) => s.setActiveSessionId)

  const openToolTab = (tab: RightPanelTab) => {
    setRoute("chat")
    const sessionId = useAppStore.getState().activeSessionId
    if (!sessionId) return
    openToolBesideChat(sessionId, tab)
  }

  const handleSelectSession = async (id: string) => {
    try {
      await resumeSession(id)
      setActiveSessionId(id)
      setRoute("chat")
    } catch (err) {
      log.error("session", "resume_session failed", {
        sessionId: id,
        error: toInvokeError(err),
      })
    }
  }

  const tabCatalog = useMemo(
    () => visibleRightPanelTabs({ hasBranchPr: true }),
    [],
  )

  const commands: CommandEntry[] = useMemo(
    () => [
      {
        id: "new-agent",
        label: "New Agent",
        icon: SquarePen,
        group: "Commands",
        hint: "⌘N",
        run: () => void newAgent(),
      },
      {
        id: "toggle-sidebar",
        label: "Toggle Sidebar",
        icon: PanelLeft,
        group: "Commands",
        hint: "⌘B",
        run: () => toggleSidebarCollapsed(),
      },
      {
        id: "toggle-right-panel",
        label: "Toggle Split View",
        icon: PanelRight,
        group: "Commands",
        hint: "⌘J",
        run: () => toggleSplit(),
      },
      ...tabCatalog.map(
        (t): CommandEntry => ({
          id: `tab-${t.id}`,
          label: `Open ${t.label} beside chat`,
          icon: t.icon,
          group: "Commands",
          run: () => openToolTab(t.id),
        }),
      ),
      {
        id: "toggle-theme",
        label: "Toggle Theme",
        icon: Moon,
        group: "Commands",
        run: () => toggleTheme(),
      },
      ...(AUTOMATIONS_UI_ENABLED
        ? ([
            {
              id: "open-automations",
              label: "Open Automations",
              icon: Bot,
              group: "Commands",
              run: () => setRoute("automations"),
            },
          ] satisfies CommandEntry[])
        : []),
      {
        id: "open-memory",
        label: "Open Memory",
        icon: Brain,
        group: "Commands",
        run: () => setRoute("memory"),
      },
      {
        id: "open-customize",
        label: "Open Customize",
        icon: SlidersHorizontal,
        group: "Commands",
        run: () => setRoute("customize"),
      },
      {
        id: "open-settings",
        label: "Open Settings",
        icon: SettingsIcon,
        group: "Commands",
        run: () => setRoute("settings"),
      },
      {
        id: "mode-agent",
        label: "Mode: Agent",
        icon: Sparkles,
        group: "Commands",
        run: () => {
          setRoute("chat")
          setComposerMode("agent")
        },
      },
      {
        id: "mode-plan",
        label: "Mode: Plan",
        icon: MessagesSquare,
        group: "Commands",
        run: () => {
          setRoute("chat")
          setComposerMode("plan")
          useAppStore.getState().revealPlanPanel()
        },
      },
      {
        id: "mode-ask",
        label: "Mode: Ask",
        icon: MessagesSquare,
        group: "Commands",
        run: () => {
          setRoute("chat")
          setComposerMode("ask")
        },
      },
      {
        id: "mode-debug",
        label: "Mode: Debug",
        icon: Bug,
        group: "Commands",
        run: () => {
          setRoute("chat")
          setComposerMode("debug")
        },
      },
      ...(FLEX_MODE_ENABLED
        ? [
            {
              id: "mode-flex",
              label: "Mode: Flex",
              icon: Network,
              group: "Commands",
              run: () => {
                setRoute("chat")
                setComposerMode("flex")
              },
            } satisfies CommandEntry,
          ]
        : []),
      ...sessions.map(
        (session): CommandEntry => ({
          id: `session:${session.id}`,
          label: sessionLabel(session),
          hint: basename(session.cwd || "~"),
          group: "Sessions",
          run: () => void handleSelectSession(session.id),
        }),
      ),
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sessions, tabCatalog],
  )

  const filtered = useMemo(() => {
    const q = query.trim()
    if (!q) return commands
    return commands
      .map((c) => ({ c, score: fuzzyScore(q, c.label) }))
      .filter((r): r is { c: CommandEntry; score: number } => r.score !== null)
      .sort((a, b) => a.score - b.score)
      .map((r) => r.c)
  }, [commands, query])

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

  useEffect(() => {
    if (!open) return

    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault()
        onClose()
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
        if (entry) {
          entry.run()
          onClose()
        }
      }
    }

    window.addEventListener("keydown", handleKey)
    return () => window.removeEventListener("keydown", handleKey)
  }, [open, onClose, filtered, activeIndex])

  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>(
      `[data-index="${activeIndex}"]`,
    )
    el?.scrollIntoView({ block: "nearest" })
  }, [activeIndex])

  if (!open) return null

  const groups: Array<{ label: CommandEntry["group"]; items: CommandEntry[] }> = (
    [
      { label: "Commands", items: filtered.filter((c) => c.group === "Commands") },
      { label: "Sessions", items: filtered.filter((c) => c.group === "Sessions") },
    ] as const
  ).filter((g) => g.items.length > 0)

  let runningIndex = -1

  return createPortal(
    <div
      className="fixed inset-0 z-[300] flex justify-center bg-black/20 animate-backdrop-in"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose()
      }}
    >
      <div
        className={cn(
          "mt-[10vh] flex h-fit w-[560px] max-w-[90vw] flex-col overflow-hidden",
          "rounded-lg bg-panel shadow-[var(--shadow-popover)] animate-tray-in",
        )}
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
      >
        <div className="flex items-center gap-1.5 border-b border-stroke-3 px-3 py-2.5">
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Type a command or search sessions…"
            aria-label="Command palette input"
            className="w-full bg-transparent text-base text-ink outline-none placeholder:text-ink-faint"
          />
        </div>

        <div ref={listRef} className="max-h-[320px] overflow-y-auto py-1">
          {groups.length === 0 ? (
            <p className="px-3 py-6 text-center text-sm text-ink-faint">
              No matching commands
            </p>
          ) : (
            groups.map((group) => (
              <div key={group.label} className="py-1">
                <p className="px-3 py-1 text-xs font-medium text-ink-faint">
                  {group.label}
                </p>
                {group.items.map((entry) => {
                  runningIndex += 1
                  const index = runningIndex
                  return (
                    <CommandPaletteRow
                      key={entry.id}
                      index={index}
                      active={index === activeIndex}
                      label={entry.label}
                      hint={entry.hint}
                      icon={entry.icon}
                      onHover={() => setActiveIndex(index)}
                      onActivate={() => {
                        entry.run()
                        onClose()
                      }}
                    />
                  )
                })}
              </div>
            ))
          )}
        </div>
      </div>
    </div>,
    document.body,
  )
}

import { useEffect, useMemo, useState } from "react"
import {
  Bot,
  Brain,
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
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
} from "@/components/ui/command"

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
    if (open) setQuery("")
  }, [open])

  const groups: Array<{ label: CommandEntry["group"]; items: CommandEntry[] }> =
    (
      [
        {
          label: "Commands",
          items: filtered.filter((c) => c.group === "Commands"),
        },
        {
          label: "Sessions",
          items: filtered.filter((c) => c.group === "Sessions"),
        },
      ] as const
    ).filter((g) => g.items.length > 0)

  return (
    <CommandDialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose()
      }}
      title="Command palette"
      description="Type a command or search sessions"
      className={cn("shadow-[var(--shadow-popover)]")}
    >
      <Command
        shouldFilter={false}
        className="rounded-lg bg-panel"
      >
        <CommandInput
          value={query}
          onValueChange={setQuery}
          placeholder="Type a command or search sessions…"
          aria-label="Command palette input"
        />
        <CommandList className="py-1">
          <CommandEmpty>No matching commands</CommandEmpty>
          {groups.map((group) => (
            <CommandGroup key={group.label} heading={group.label}>
              {group.items.map((entry) => {
                const Icon = entry.icon
                return (
                  <CommandItem
                    key={entry.id}
                    value={entry.id}
                    onSelect={() => {
                      entry.run()
                      onClose()
                    }}
                  >
                    {Icon ? (
                      <Icon
                        className="size-3.5 shrink-0 text-ink-muted"
                        aria-hidden
                      />
                    ) : (
                      <span className="size-3.5 shrink-0" />
                    )}
                    <span className="min-w-0 flex-1 truncate">{entry.label}</span>
                    {entry.hint ? (
                      <CommandShortcut>{entry.hint}</CommandShortcut>
                    ) : null}
                  </CommandItem>
                )
              })}
            </CommandGroup>
          ))}
        </CommandList>
      </Command>
    </CommandDialog>
  )
}

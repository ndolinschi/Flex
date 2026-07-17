import {
  Bot,
  Brain,
  Bug,
  Cog,
  CreditCard,
  Component,
  Database,
  Palette,
  Plug,
  Search,
  X,
} from "@/components/icons"
import { TabsList, TabsTrigger } from "@/components/ui/tabs"
import { AUTOMATIONS_UI_ENABLED } from "../../lib/featureFlags"
import type { SettingsSearchEntry, SettingsSectionId } from "../../lib/settingsSearchIndex"
import { cn } from "../../lib/utils"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group"

export type SettingsNavItem = {
  id: SettingsSectionId
  label: string
}

/** Nav order + labels for our section set (see DESIGN.md Settings nav). */
const ALL_SETTINGS_NAV_ITEMS: SettingsNavItem[] = [
  { id: "general", label: "General" },
  { id: "appearance", label: "Appearance" },
  { id: "models", label: "Models & Connections" },
  { id: "behavior", label: "Behavior" },
  { id: "memory", label: "Memory" },
  { id: "indexing", label: "Indexing" },
  { id: "tools-mcp", label: "Tools & MCP" },
  { id: "automations", label: "Automations" },
  { id: "diagnostics", label: "Diagnostics" },
]

/** Visible settings nav — Automations gated by `AUTOMATIONS_UI_ENABLED`. */
export const SETTINGS_NAV_ITEMS: SettingsNavItem[] = ALL_SETTINGS_NAV_ITEMS.filter(
  (item) => item.id !== "automations" || AUTOMATIONS_UI_ENABLED,
)

/** Icon per section — mapped from the reference `Bii` icon dictionary
 * (§7) onto local Lucide icons (`@/components/icons`) already used elsewhere. */
const NAV_ICONS: Record<SettingsSectionId, typeof Cog> = {
  general: Cog,
  appearance: Palette,
  models: Component,
  behavior: CreditCard,
  memory: Brain,
  indexing: Database,
  "tools-mcp": Plug,
  diagnostics: Bug,
  automations: Bot,
}

type SettingsNavProps = {
  query: string
  onQueryChange: (query: string) => void
  results: SettingsSearchEntry[]
  resultIndex: number
  onResultIndexChange: (index: number) => void
  onResultSelect: (entry: SettingsSearchEntry) => void
}

/** Persistent left nav (see DESIGN.md Settings shell / nav): width
 * clamp(100px,25%,200px), search-at-top that swaps the whole nav list for a
 * flat result list once the query is non-empty (navigate-to-result, not
 * inline filtering). Section list is a vertical shadcn `TabsList` (must sit
 * under `SettingsShell`'s `Tabs` root); search results replace the list and
 * leave Tabs value control to the shell. */
export const SettingsNav = ({
  query,
  onQueryChange,
  results,
  resultIndex,
  onResultIndexChange,
  onResultSelect,
}: SettingsNavProps) => {
  const searching = query.trim().length > 0

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (!searching || results.length === 0) {
      if (e.key === "Escape") onQueryChange("")
      return
    }
    if (e.key === "ArrowDown") {
      e.preventDefault()
      onResultIndexChange((resultIndex + 1) % results.length)
    } else if (e.key === "ArrowUp") {
      e.preventDefault()
      onResultIndexChange((resultIndex - 1 + results.length) % results.length)
    } else if (e.key === "Enter") {
      e.preventDefault()
      const entry = results[resultIndex]
      if (entry) onResultSelect(entry)
    } else if (e.key === "Escape") {
      e.preventDefault()
      onQueryChange("")
    }
  }

  return (
    <nav
      className="sticky top-0 flex max-h-full shrink-0 flex-col gap-3 self-start pt-6"
      style={{ width: "clamp(100px, 25%, 200px)" }}
    >
      <div className="px-1">
        <InputGroup className="h-7">
          <InputGroupAddon align="inline-start" className="pl-2 text-ink-faint">
            <Search className="size-3" aria-hidden />
          </InputGroupAddon>
          <InputGroupInput
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Search Settings"
            aria-label="Search Settings"
            className="h-7 px-0 text-xs"
          />
          {searching ? (
            <InputGroupAddon align="inline-end" className="pr-1">
              <InputGroupButton
                type="button"
                size="icon-xs"
                variant="ghost"
                aria-label="Clear search"
                onClick={() => onQueryChange("")}
                className="size-5 text-ink-faint hover:text-ink"
              >
                <X className="size-3" aria-hidden />
              </InputGroupButton>
            </InputGroupAddon>
          ) : null}
        </InputGroup>
      </div>

      {searching ? (
        <div className="flex flex-col gap-0.5 px-1" role="listbox" aria-label="Search results">
          {results.length === 0 ? (
            <p className="px-1.5 py-1 text-xs text-ink-faint">No results found.</p>
          ) : (
            results.map((entry, i) => (
              <button
                key={`${entry.section}-${entry.rowId}`}
                type="button"
                role="option"
                aria-selected={i === resultIndex}
                onClick={() => onResultSelect(entry)}
                onMouseEnter={() => onResultIndexChange(i)}
                className={cn(
                  "flex flex-col gap-0 rounded-sm px-1.5 py-1 text-left transition-colors duration-[var(--duration-fast)]",
                  i === resultIndex ? "bg-fill-2 text-ink" : "hover:bg-fill-4",
                )}
              >
                <span className="truncate text-sm leading-4 text-ink-secondary">{entry.title}</span>
                {entry.description ? (
                  <span className="truncate text-xs text-ink-faint">
                    {entry.description}
                  </span>
                ) : null}
              </button>
            ))
          )}
        </div>
      ) : (
        <TabsList
          variant="line"
          aria-label="Settings sections"
          className="h-auto w-full flex-col items-stretch gap-1.5 rounded-none bg-transparent p-0 px-1"
        >
          {SETTINGS_NAV_ITEMS.map((item) => {
            const Icon = NAV_ICONS[item.id]
            return (
              <TabsTrigger
                key={item.id}
                value={item.id}
                className={cn(
                  "h-auto flex-none justify-start gap-1.5 rounded-sm border-transparent px-1.5 py-1 text-sm leading-4 font-normal",
                  "text-ink-secondary after:hidden",
                  "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
                  "hover:bg-fill-4 hover:text-ink-secondary",
                  "data-active:bg-fill-2 data-active:text-ink data-active:shadow-none",
                  "dark:data-active:border-transparent dark:data-active:bg-fill-2",
                )}
              >
                <Icon className="h-3.5 w-3.5 shrink-0" aria-hidden />
                <span className="truncate">{item.label}</span>
              </TabsTrigger>
            )
          })}
        </TabsList>
      )}
    </nav>
  )
}

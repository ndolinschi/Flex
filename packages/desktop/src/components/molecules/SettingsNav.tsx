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
  Radio,
  Search,
  X,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { TextInput } from "../atoms"
import { AUTOMATIONS_UI_ENABLED } from "../../lib/featureFlags"
import type { SettingsSearchEntry, SettingsSectionId } from "../../lib/settingsSearchIndex"
import { cn } from "../../lib/utils"

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
  { id: "remote-access", label: "Remote Access" },
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
 * (§7) onto lucide-react equivalents already used elsewhere in this app. */
const NAV_ICONS: Record<SettingsSectionId, typeof Cog> = {
  general: Cog,
  appearance: Palette,
  models: Component,
  behavior: CreditCard,
  "remote-access": Radio,
  memory: Brain,
  indexing: Database,
  "tools-mcp": Plug,
  diagnostics: Bug,
  automations: Bot,
}

type SettingsNavProps = {
  active: SettingsSectionId
  onSelect: (id: SettingsSectionId) => void
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
 * inline filtering). */
export const SettingsNav = ({
  active,
  onSelect,
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
      <div className="relative px-1">
        <Search
          className="pointer-events-none absolute left-2.5 top-1/2 h-3 w-3 -translate-y-1/2 text-ink-faint"
          aria-hidden
        />
        <TextInput
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Search Settings"
          aria-label="Search Settings"
          className="h-7 pl-7 pr-7 text-xs"
        />
        {searching ? (
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label="Clear search"
            onClick={() => onQueryChange("")}
            className="absolute right-2 top-1/2 -translate-y-1/2 text-ink-faint hover:bg-transparent hover:text-ink"
          >
            <X aria-hidden />
          </Button>
        ) : null}
      </div>

      {searching ? (
        <div className="flex flex-col gap-0.5 px-1" role="listbox" aria-label="Search results">
          {results.length === 0 ? (
            <p className="px-1.5 py-1 text-xs text-ink-faint">No results found.</p>
          ) : (
            results.map((entry, i) => (
              <Button
                key={`${entry.section}-${entry.rowId}`}
                variant="ghost"
                role="option"
                aria-selected={i === resultIndex}
                onClick={() => onResultSelect(entry)}
                onMouseEnter={() => onResultIndexChange(i)}
                className={cn(
                  "h-auto w-full flex-col items-start gap-0 rounded-sm px-1.5 py-1 text-left",
                  i === resultIndex ? "bg-fill-2 text-ink" : "hover:bg-fill-4",
                )}
              >
                <span className="truncate text-sm leading-4 text-ink-secondary">{entry.title}</span>
                {entry.description ? (
                  <span className="truncate text-xs text-ink-faint">
                    {entry.description}
                  </span>
                ) : null}
              </Button>
            ))
          )}
        </div>
      ) : (
        <div className="flex flex-col gap-1.5 px-1">
          {SETTINGS_NAV_ITEMS.map((item) => {
            const Icon = NAV_ICONS[item.id]
            const isActive = item.id === active
            return (
              <Button
                key={item.id}
                variant="ghost"
                onClick={() => onSelect(item.id)}
                aria-current={isActive ? "page" : undefined}
                className={cn(
                  "h-auto w-full justify-start gap-1.5 rounded-sm px-1.5 py-1 text-sm leading-4",
                  "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
                  isActive
                    ? "bg-fill-2 text-ink hover:bg-fill-2"
                    : "text-ink-secondary hover:bg-fill-4",
                )}
              >
                <Icon className="h-3.5 w-3.5 shrink-0" aria-hidden />
                <span className="truncate">{item.label}</span>
              </Button>
            )
          })}
        </div>
      )}
    </nav>
  )
}

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
} from "lucide-react"
import { TextInput } from "../atoms"
import type { SettingsSearchEntry, SettingsSectionId } from "../../lib/settingsSearchIndex"
import { cn } from "../../lib/utils"

export type SettingsNavItem = {
  id: SettingsSectionId
  label: string
}

/** Nav order + labels for our section set (design-map/07-settings.md §7's
 * `Aml` order / `kci` labels, narrowed to the sections this build actually
 * houses — see report for the full Customize-redistribution mapping). */
export const SETTINGS_NAV_ITEMS: SettingsNavItem[] = [
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

/** Icon per section — mapped from the reference `Bii` icon dictionary
 * (§7) onto lucide-react equivalents already used elsewhere in this app. */
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
  active: SettingsSectionId
  onSelect: (id: SettingsSectionId) => void
  query: string
  onQueryChange: (query: string) => void
  results: SettingsSearchEntry[]
  resultIndex: number
  onResultIndexChange: (index: number) => void
  onResultSelect: (entry: SettingsSearchEntry) => void
}

/** Persistent left nav (design-map/07-settings.md §1-3): width
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
      className="sticky top-0 flex max-h-full shrink-0 flex-col gap-3 self-start pt-12"
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
          <button
            type="button"
            aria-label="Clear search"
            onClick={() => onQueryChange("")}
            className="absolute right-2 top-1/2 -translate-y-1/2 text-ink-faint hover:text-ink"
          >
            <X className="h-3 w-3" aria-hidden />
          </button>
        ) : null}
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
                  "flex flex-col gap-0 rounded-sm px-1.5 py-1 text-left transition-colors",
                  i === resultIndex ? "bg-fill-4" : "hover:bg-fill-4",
                )}
              >
                <span className="truncate text-[12px] leading-4 text-ink-secondary">{entry.title}</span>
                {entry.description ? (
                  <span className="truncate text-[11px] text-ink-faint">
                    {entry.description}
                  </span>
                ) : null}
              </button>
            ))
          )}
        </div>
      ) : (
        <div className="flex flex-col gap-1.5 px-1">
          {SETTINGS_NAV_ITEMS.map((item) => {
            const Icon = NAV_ICONS[item.id]
            const isActive = item.id === active
            return (
              <button
                key={item.id}
                type="button"
                onClick={() => onSelect(item.id)}
                aria-current={isActive ? "page" : undefined}
                className={cn(
                  "flex items-center gap-1.5 rounded-sm px-1.5 py-1 text-[12px] leading-4 transition-colors",
                  isActive
                    ? "bg-fill-4 text-ink"
                    : "text-ink-secondary hover:bg-fill-4",
                )}
              >
                <Icon className="h-3.5 w-3.5 shrink-0" aria-hidden />
                <span className="truncate">{item.label}</span>
              </button>
            )
          })}
        </div>
      )}
    </nav>
  )
}

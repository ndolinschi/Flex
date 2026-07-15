import type { ReactNode } from "react"
import { ChevronRight, type LucideIcon } from "lucide-react"
import { DiffStat } from "../../atoms"
import { cn } from "../../../lib/utils"
import type { RightPanelTab } from "../../../stores/appStore"
import { PROJECT_PINNED_TABS, type RightPanelTabDef } from "./tabs"

export type RightPanelMiniTabsProps = {
  openTabDefs: RightPanelTabDef[]
  selectedTab: RightPanelTab
  changesTotals: { added: number; removed: number }
  terminalCount: number
  catalog: RightPanelTabDef[]
  onSelectTab: (id: RightPanelTab) => void
}

type MiniRowProps = {
  icon: LucideIcon
  label: string
  selected?: boolean
  trailing?: ReactNode
  onClick: () => void
}

const MiniRow = ({
  icon: Icon,
  label,
  selected = false,
  trailing,
  onClick,
}: MiniRowProps) => (
  <button
    type="button"
    onClick={onClick}
    className={cn(
      "flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-sm",
      "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
      selected
        ? "bg-fill-2 text-ink"
        : "text-ink-secondary hover:bg-fill-4 hover:text-ink",
    )}
  >
    <Icon className="h-3.5 w-3.5 shrink-0 text-icon-3" aria-hidden />
    <span className="min-w-0 flex-1 truncate">{label}</span>
    {trailing}
  </button>
)

const rowLabel = (
  t: RightPanelTabDef,
  terminalCount: number,
): string => {
  if (t.id === "terminal" && terminalCount > 0) {
    return terminalCount === 1 ? "1 Terminal" : `${terminalCount} Terminal`
  }
  return t.label
}

const rowTrailing = (
  t: RightPanelTabDef,
  changesTotals: { added: number; removed: number },
  terminalCount: number,
): ReactNode => {
  if (
    t.id === "changes" &&
    (changesTotals.added > 0 || changesTotals.removed > 0)
  ) {
    return <DiffStat summary={changesTotals} size="xs" className="shrink-0" />
  }
  if (t.id === "terminal" && terminalCount > 0) {
    return (
      <ChevronRight className="h-3.5 w-3.5 shrink-0 text-icon-3" aria-hidden />
    )
  }
  return undefined
}

/** Cursor-style mini rows when the right panel is closed — no section chrome. */
export const RightPanelMiniTabs = ({
  openTabDefs,
  selectedTab,
  changesTotals,
  terminalCount,
  catalog,
  onSelectTab,
}: RightPanelMiniTabsProps) => {
  const openIds = new Set(openTabDefs.map((t) => t.id))
  const pinnedDefs = PROJECT_PINNED_TABS.map((id) =>
    catalog.find((t) => t.id === id),
  ).filter((t): t is RightPanelTabDef => t != null && !openIds.has(t.id))
  // Open session tabs first (selection highlight), then pinned shortcuts not
  // already open — one flat list without section headers.
  const rows = [...openTabDefs, ...pinnedDefs]

  return (
    <nav
      aria-label="Details panel shortcuts"
      className={cn(
        "absolute right-2 z-20 w-[200px]",
        "top-[calc(var(--header-height)+0.5rem)]",
        "flex flex-col gap-0.5 px-1.5 py-1.5",
      )}
    >
      {rows.map((t) => (
        <MiniRow
          key={t.id}
          icon={t.icon}
          label={rowLabel(t, terminalCount)}
          selected={selectedTab === t.id && openIds.has(t.id)}
          trailing={rowTrailing(t, changesTotals, terminalCount)}
          onClick={() => onSelectTab(t.id)}
        />
      ))}
    </nav>
  )
}

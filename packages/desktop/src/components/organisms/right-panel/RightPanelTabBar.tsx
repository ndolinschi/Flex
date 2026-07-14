import type { MouseEvent as ReactMouseEvent } from "react"
import { Plus, X } from "lucide-react"
import { DiffStat, IconButton, Tab, TabStrip } from "../../atoms"
import type { RightPanelTab } from "../../../stores/appStore"
import { TABS } from "./tabs"

export type RightPanelTabDef = (typeof TABS)[number]

export type RightPanelTabBarProps = {
  openTabDefs: RightPanelTabDef[]
  closableTabDefs: RightPanelTabDef[]
  tab: RightPanelTab
  narrow: boolean
  changesCount: number | undefined
  changesTotals: { added: number; removed: number }
  onSelectTab: (id: RightPanelTab) => void
  onCloseTab: (id: RightPanelTab) => void
  onOpenAddMenu: (e: ReactMouseEvent<HTMLButtonElement>) => void
  onClosePanel: () => void
}

/** Open-tabs strip for the right panel header (select / close / add).
 * Wide layout: open/close is AppHeader's single ⌘J / PanelRight toggle
 * (Cursor-style — no second collapse control here). Narrow overlay keeps
 * an explicit close because the header toggle sits under the panel. */
export const RightPanelTabBar = ({
  openTabDefs,
  closableTabDefs,
  tab,
  narrow,
  changesCount,
  changesTotals,
  onSelectTab,
  onCloseTab,
  onOpenAddMenu,
  onClosePanel,
}: RightPanelTabBarProps) => {
  return (
    <TabStrip data-browser-chrome="tabs">
      {openTabDefs.map((t) => (
        <Tab
          key={t.id}
          selected={tab === t.id}
          icon={t.icon ? <t.icon aria-hidden /> : undefined}
          badge={
            t.id === "changes" && changesCount ? (
              <DiffStat summary={changesTotals} />
            ) : undefined
          }
          onSelect={() => onSelectTab(t.id)}
          onClose={() => onCloseTab(t.id)}
          closeLabel={`Close ${t.label}`}
        >
          {t.label}
        </Tab>
      ))}

      {closableTabDefs.length > 0 ? (
        <IconButton
          label="Open tab"
          onClick={onOpenAddMenu}
          className="h-6 w-6"
        >
          <Plus className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      ) : null}

      {narrow ? (
        <IconButton
          label="Close panel"
          onClick={onClosePanel}
          className="ml-auto"
        >
          <X className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      ) : null}
    </TabStrip>
  )
}

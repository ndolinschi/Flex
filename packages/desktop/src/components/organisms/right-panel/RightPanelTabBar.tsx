import type { MouseEvent as ReactMouseEvent } from "react"
import { ChevronsRight, Plus, X } from "lucide-react"
import { DiffStat, IconButton } from "../../atoms"
import { cn } from "../../../lib/utils"
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
  onCollapse: () => void
  onClosePanel: () => void
}

/** Open-tabs strip for the right panel header (select / close / add / collapse). */
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
  onCollapse,
  onClosePanel,
}: RightPanelTabBarProps) => {
  return (
    <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1 border-b border-stroke-3 px-2" data-browser-chrome="tabs">
      {openTabDefs.map((t) => (
        <button
          key={t.id}
          type="button"
          onClick={() => onSelectTab(t.id)}
          aria-selected={tab === t.id}
          role="tab"
          className={cn(
            "group flex h-7 items-center gap-1.5 rounded-lg px-2 text-sm",
            "tracking-[var(--tracking-caption)]",
            "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
            tab === t.id
              ? "bg-fill-3 text-ink"
              : "text-ink-muted hover:bg-fill-3 hover:text-ink-secondary",
          )}
        >
          {t.icon ? <t.icon className="h-3.5 w-3.5" aria-hidden /> : null}
          {t.label}
          {t.id === "changes" && changesCount ? (
            <DiffStat summary={changesTotals} />
          ) : null}
          {/* Close-on-hover — never destroys the underlying
           * terminal PTY / browser webview, only hides the tab (see
           * handleCloseTab). Collapses to zero width at rest (no reserved
           * gap) and expands + fades in only while the tab is hovered,
           * matching the SessionListItem hover-actions idiom. */}
          <span
            role="button"
            aria-label={`Close ${t.label}`}
            tabIndex={-1}
            onClick={(e) => {
              e.stopPropagation()
              onCloseTab(t.id)
            }}
            className={cn(
              "ml-0 max-w-0 shrink-0 overflow-hidden rounded-sm p-0 opacity-0",
              "transition-[max-width,margin,padding,opacity] duration-[140ms] ease-[var(--easing-default)]",
              "hover:bg-fill-1 group-hover:ml-0.5 group-hover:max-w-[1rem] group-hover:p-0.5 group-hover:opacity-100",
            )}
          >
            <X className="h-3 w-3" aria-hidden />
          </span>
        </button>
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

      {!narrow ? (
        <IconButton
          label="Collapse panel"
          onClick={onCollapse}
          className="ml-auto"
        >
          <ChevronsRight className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      ) : (
        // Full-width overlay only — wide mode has no header close button
        // (AppHeader's ⌘J toggle covers it there) and must stay
        // byte-identical; at narrow the panel fills the chat area so a
        // backdrop click alone is undiscoverable.
        <IconButton
          label="Close panel"
          onClick={onClosePanel}
          className="ml-auto"
        >
          <X className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      )}
    </div>
  )
}

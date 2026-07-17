import { useEffect, useRef, useState, type ReactNode } from "react"
import { ArrowLeft } from "@/components/icons"
import { Tabs, TabsContent } from "@/components/ui/tabs"
import { IconButton } from "../atoms"
import { SettingsNav, SETTINGS_NAV_ITEMS } from "../molecules"
import { AUTOMATIONS_UI_ENABLED } from "../../lib/featureFlags"
import type { SettingsSectionId } from "../../lib/settingsSearchIndex"
import { searchSettings, type SettingsSearchEntry } from "../../lib/settingsSearchIndex"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"

type SettingsShellProps = {
  /** Section content keyed by id — the shell renders whichever one is
   * active, so the header title/description below are per-section, not a
   * single title for the whole surface (DESIGN.md Settings). */
  sections: Partial<Record<SettingsSectionId, ReactNode>>
  /** Page title shown above the active section's content. */
  titleFor: (section: SettingsSectionId) => string
  descriptionFor?: (section: SettingsSectionId) => string | undefined
  /** When true, omit the outer full-window chrome (used inside AppShell). */
  embedded?: boolean
}

/** Settings shell — persistent left nav + content pane (see DESIGN.md
 * Settings shell / nav). Replaces the old single-page "Back to chat" header
 * shell: this is now the one surface that houses General / Appearance /
 * Models & Connections / Behavior / Memory / Tools & MCP / Automations, with
 * a Search Settings box at the top of the nav that navigates to (and
 * pulse-highlights) a row in another section rather than filtering in
 * place. Section switching uses vertical shadcn Tabs; search still swaps
 * the nav list for results without fighting Tabs value control. */
export const SettingsShell = ({
  sections,
  titleFor,
  descriptionFor,
  embedded = false,
}: SettingsShellProps) => {
  const setRoute = useAppStore((s) => s.setRoute)
  const activeSection = useAppStore((s) => s.settingsSection)
  const setActiveSection = useAppStore((s) => s.setSettingsSection)

  const [query, setQuery] = useState("")
  const [resultIndex, setResultIndex] = useState(0)
  const [highlightRowId, setHighlightRowId] = useState<string | null>(null)
  const contentRef = useRef<HTMLDivElement>(null)
  const highlightTimeoutRef = useRef<number | null>(null)

  const results = searchSettings(query)

  useEffect(() => {
    if (activeSection === "automations" && !AUTOMATIONS_UI_ENABLED) {
      setActiveSection("general")
    }
  }, [activeSection, setActiveSection])

  useEffect(() => {
    setResultIndex(0)
  }, [query])

  const navigateToResult = (entry: SettingsSearchEntry) => {
    setActiveSection(entry.section)
    setQuery("")
    // Wait a tick for the section to mount/switch before scrolling+highlighting.
    window.requestAnimationFrame(() => {
      const row = contentRef.current?.querySelector<HTMLElement>(
        `[data-settings-row="${entry.rowId}"]`,
      )
      if (!row) return
      row.scrollIntoView({ block: "center", behavior: "smooth" })
      if (highlightTimeoutRef.current) window.clearTimeout(highlightTimeoutRef.current)
      setHighlightRowId(entry.rowId)
      highlightTimeoutRef.current = window.setTimeout(() => {
        setHighlightRowId(null)
      }, 1_500)
    })
  }

  useEffect(() => {
    return () => {
      if (highlightTimeoutRef.current) window.clearTimeout(highlightTimeoutRef.current)
    }
  }, [])

  const body = (
    <>
      <header className="flex h-[var(--header-height)] shrink-0 items-center gap-2 border-b border-stroke-3 px-4">
        <IconButton label="Back to chat" onClick={() => setRoute("chat")}>
          <ArrowLeft className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      </header>
      <Tabs
        orientation="vertical"
        value={activeSection}
        onValueChange={(value) => {
          setActiveSection(value as SettingsSectionId)
          setQuery("")
        }}
        className="flex flex-1 items-stretch gap-6 overflow-y-auto px-4"
      >
        <SettingsNav
          query={query}
          onQueryChange={setQuery}
          results={results}
          resultIndex={resultIndex}
          onResultIndexChange={setResultIndex}
          onResultSelect={navigateToResult}
        />
        <div className="min-w-0 flex-1">
          <div className="mb-5 flex items-start gap-5 pt-6">
            <div className="min-w-0">
              <h1 className="text-[17px] font-medium leading-[21px] text-ink">
                {titleFor(activeSection)}
              </h1>
              {descriptionFor?.(activeSection) ? (
                <p className="mt-1 text-base leading-[18px] text-ink-secondary">
                  {descriptionFor(activeSection)}
                </p>
              ) : null}
            </div>
          </div>
          <div
            ref={contentRef}
            className={cn(
              "flex flex-col gap-3 pb-12",
              "[&_[data-settings-row]]:transition-shadow",
            )}
          >
            {SETTINGS_NAV_ITEMS.map((item) => {
              const content = sections[item.id]
              if (!content) return null
              return (
                <TabsContent
                  key={item.id}
                  value={item.id}
                  className="mt-0 outline-none"
                >
                  <HighlightScope rowId={highlightRowId}>{content}</HighlightScope>
                </TabsContent>
              )
            })}
          </div>
        </div>
      </Tabs>
    </>
  )

  if (embedded) {
    return <div className="flex h-full min-w-0 flex-1 flex-col bg-bg">{body}</div>
  }

  return <div className="flex h-full flex-col bg-bg">{body}</div>
}

/** Applies `.animate-settings-row-highlight` to whichever DOM node currently
 * carries `data-settings-row={rowId}` — done as a small effect rather than
 * threading highlight state through every section, so sections stay plain
 * `SettingRow` trees with no highlight-awareness of their own. */
const HighlightScope = ({
  rowId,
  children,
}: {
  rowId: string | null
  children: ReactNode
}) => {
  const scopeRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const scope = scopeRef.current
    if (!scope || !rowId) return
    const row = scope.querySelector<HTMLElement>(`[data-settings-row="${rowId}"]`)
    if (!row) return
    row.classList.add("animate-settings-row-highlight")
    return () => {
      row.classList.remove("animate-settings-row-highlight")
    }
  }, [rowId])

  return (
    <div ref={scopeRef} className="contents">
      {children}
    </div>
  )
}

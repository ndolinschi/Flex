import { useEffect, useRef, useState, type ReactNode } from "react"
import { Button } from "@/components/ui/button"
import { ArrowLeft } from "lucide-react"
import { SettingsNav } from "../molecules"
import { AUTOMATIONS_UI_ENABLED } from "../../lib/featureFlags"
import type { SettingsSectionId } from "../../lib/settingsSearchIndex"
import { searchSettings, type SettingsSearchEntry } from "../../lib/settingsSearchIndex"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"

type SettingsShellProps = {
  sections: Partial<Record<SettingsSectionId, ReactNode>>
  titleFor: (section: SettingsSectionId) => string
  descriptionFor?: (section: SettingsSectionId) => string | undefined
  embedded?: boolean
}

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

  const activeContent = sections[activeSection]

  const body = (
    <>
      <header className="flex h-[var(--header-height)] shrink-0 items-center gap-2 border-b border-stroke-3 px-4">
        <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Back to chat" title="Back to chat"
      onClick={() => setRoute("chat")}
      className={cn(
        "text-ink-muted hover:bg-fill-4 hover:text-ink",
        "opacity-50 hover:opacity-80",
        "h-6 w-6",
      )}
    >
      <ArrowLeft className="h-3.5 w-3.5" aria-hidden />
    </Button>
      </header>
      <main className="flex flex-1 items-stretch gap-6 overflow-y-auto px-4">
        <SettingsNav
          active={activeSection}
          onSelect={(id) => {
            setActiveSection(id)
            setQuery("")
          }}
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
              "[&_[data-settings-row]]:transition-shadow [&_[data-settings-row]]:duration-[var(--duration-fast)]",
            )}
          >
            <HighlightScope rowId={highlightRowId}>{activeContent}</HighlightScope>
          </div>
        </div>
      </main>
    </>
  )

  if (embedded) {
    return <div className="flex h-full min-w-0 flex-1 flex-col bg-bg">{body}</div>
  }

  return <div className="flex h-full flex-col bg-bg">{body}</div>
}

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

import type { CSSProperties, ReactNode } from "react"
import { cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import { Button } from "@/components/ui/button"

const QUICKSTART_SUGGESTIONS = [
  "Fix a bug in this repo",
  "Explain the architecture",
  "Add tests for recent changes",
]

type ChatShellProps = {
  sidebar?: ReactNode
  hideSidebar?: boolean
  timeline: ReactNode
  composer: ReactNode
  overlay?: ReactNode
  /** True for floating overlays that should sit flush above the composer
   * section. Prefer `Composer.dockedOverlay` for Permission/Question cards
   * that must merge with the bubble (avoids a page-bg gap at the seam). */
  overlayDocked?: boolean
  /** Empty conversation — compact top-weighted empty, composer stays docked. */
  composerHero?: boolean
  heroTitle?: string
  heroHint?: string
}

/**
 * Chat column shell. Composer always docks at the bottom (IDE rail), even
 * when the conversation is empty — never a centered marketing hero.
 */
export const ChatShell = ({
  sidebar,
  hideSidebar = false,
  timeline,
  composer,
  overlay,
  overlayDocked = false,
  composerHero = false,
  heroTitle = "Agent",
  heroHint = "Describe a task to start the native agent loop.",
}: ChatShellProps) => {
  const setComposerDraft = useAppStore((s) => s.setComposerDraft)
  const tight = useAppStore((s) => s.viewport === "tight")
  const wide = useAppStore((s) => s.viewport === "wide")

  const handleQuickstart = (text: string) => {
    setComposerDraft(text)
    window.requestAnimationFrame(() => {
      const el = document.querySelector<HTMLTextAreaElement>("[data-composer]")
      el?.focus()
    })
  }

  const paneStyle = {
    ...(tight ? ({ "--content-rail": "100%" } as CSSProperties) : {}),
  } as CSSProperties

  const pane = (
    <div
      className={cn(
        "flex h-full min-h-0 min-w-0 flex-1 flex-col bg-bg",
        wide && "min-w-[380px]",
      )}
      style={Object.keys(paneStyle).length > 0 ? paneStyle : undefined}
    >
      <main className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          {composerHero ? (
            <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-3 pt-6">
              <div className="mx-auto w-full max-w-[var(--content-rail)]">
                <h2 className="truncate text-sm font-medium text-ink">
                  {heroTitle}
                </h2>
                <p className="mt-1 text-xs text-ink-muted">{heroHint}</p>
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {QUICKSTART_SUGGESTIONS.map((suggestion) => (
                    <Button
                      key={suggestion}
                      variant="outline"
                      size="xs"
                      onClick={() => handleQuickstart(suggestion)}
                      className="rounded-md border-stroke-3 text-xs text-ink-secondary opacity-80 hover:border-stroke-2 hover:bg-fill-4 hover:text-ink-secondary hover:opacity-100"
                    >
                      {suggestion}
                    </Button>
                  ))}
                </div>
              </div>
            </div>
          ) : (
            timeline
          )}
        </div>

        <div className="relative z-50 shrink-0 pb-2">
          {overlay ? (
            <div
              className={cn(
                "absolute inset-x-0 bottom-full z-50 flex justify-center px-3",
                overlayDocked ? "mb-0" : "mb-3",
              )}
            >
              {overlay}
            </div>
          ) : null}
          {composer}
        </div>
      </main>
    </div>
  )

  if (hideSidebar) return pane

  return (
    <div className="flex h-full min-h-0 bg-bg">
      {sidebar}
      {pane}
    </div>
  )
}

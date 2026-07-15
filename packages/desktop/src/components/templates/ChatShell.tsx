import type { CSSProperties, ReactNode } from "react"
import { AppHeader } from "../organisms"
import { cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"

const QUICKSTART_SUGGESTIONS = [
  "Fix a bug in this repo",
  "Explain the architecture",
  "Add tests for recent changes",
]

type ChatShellProps = {
  sidebar?: ReactNode
  hideSidebar?: boolean
  /** When false, omit AppHeader (content panes own chrome). Default true. */
  showHeader?: boolean
  timeline: ReactNode
  composer: ReactNode
  overlay?: ReactNode
  /** True for floating overlays that should sit flush above the composer
   * section. Prefer `Composer.dockedOverlay` for Permission/Question cards
   * that must merge with the bubble (avoids a page-bg gap at the seam). */
  overlayDocked?: boolean
  composerHero?: boolean
  heroTitle?: string
  heroHint?: string
}

export const ChatShell = ({
  sidebar,
  hideSidebar = false,
  showHeader = true,
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
        "flex h-full min-h-0 min-w-0 flex-1 flex-col",
        wide && "min-w-[380px]",
      )}
      style={Object.keys(paneStyle).length > 0 ? paneStyle : undefined}
    >
      {showHeader ? <AppHeader /> : null}
      <main className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
        <div
          className={cn(
            "flex min-h-0 flex-col overflow-hidden",
            composerHero ? "hidden" : "min-h-0 flex-1",
          )}
        >
          {timeline}
        </div>

        {composerHero ? (
          <div className="flex min-h-0 flex-1 flex-col justify-center overflow-y-auto">
            <div className="mx-auto mb-5 w-full max-w-[var(--content-rail)] px-3 text-center">
              <h2 className="mb-4 truncate text-[28px] font-semibold leading-none tracking-[-0.04em] text-ink">
                {heroTitle}
              </h2>
              <p className="text-base text-ink-muted">{heroHint}</p>
              <div className="mt-4 flex flex-wrap justify-center gap-2">
                {QUICKSTART_SUGGESTIONS.map((suggestion) => (
                  <button
                    key={suggestion}
                    type="button"
                    onClick={() => handleQuickstart(suggestion)}
                    className="rounded-full border border-stroke-3 px-3 py-1 text-sm text-ink-secondary transition-colors duration-[var(--duration-fast)] hover:border-stroke-2 hover:bg-fill-4"
                  >
                    {suggestion}
                  </button>
                ))}
              </div>
            </div>
            {composer}
            <div className="pb-2" />
          </div>
        ) : (
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
        )}

        {composerHero && overlay ? (
          <div className="absolute inset-x-0 bottom-6 z-50 flex justify-center px-3">
            {overlay}
          </div>
        ) : null}
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

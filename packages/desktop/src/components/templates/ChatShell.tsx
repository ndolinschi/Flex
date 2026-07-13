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
  timeline: ReactNode
  composer: ReactNode
  overlay?: ReactNode
  /** True for overlays meant to read as part of the composer stack (e.g. the
   * question wizard) rather than a floating card above it: drops the gap so
   * the overlay sits flush against the composer's top edge. */
  overlayDocked?: boolean
  composerHero?: boolean
  heroTitle?: string
  heroHint?: string
}

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
  // "tight" viewport (~<680px, see hooks/useViewportWidth): tighten the chat
  // gutters. TurnTimeline/Composer both size their content rail off the
  // --content-rail custom property (`max-w-[var(--content-rail)]`), and
  // custom properties cascade to descendants, so overriding it here narrows
  // both without editing either (TurnTimeline is out of scope for this pass).
  const tight = useAppStore((s) => s.viewport === "tight")

  const handleQuickstart = (text: string) => {
    setComposerDraft(text)
    window.requestAnimationFrame(() => {
      const el = document.querySelector<HTMLTextAreaElement>("[data-composer]")
      el?.focus()
    })
  }

  // Hard CSS floor for the chat column — wide viewport only. No live-clamp
  // math (see stores/appStore.ts's CHAT_MIN_WIDTH) can ever fully hide chat:
  // this is a backstop against a sash math error, not the primary defense.
  // Narrow/tight are exempt — sidebar/right-panel are absolute overlays
  // there (see SessionSidebar.tsx / RightPanel.tsx `narrow` handling), so the
  // chat pane already renders at full width underneath them.
  const wide = useAppStore((s) => s.viewport === "wide")

  const pane = (
    <div
      className={cn(
        "flex h-full min-h-0 min-w-0 flex-1 flex-col",
        wide && "min-w-[380px]",
      )}
      style={tight ? ({ "--content-rail": "100%" } as CSSProperties) : undefined}
    >
      <AppHeader />
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
            <div
              className={cn(
                "mx-auto mb-5 w-full max-w-[var(--content-rail)] text-center",
                tight ? "px-3" : "px-4",
              )}
            >
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
                    className="rounded-full border border-stroke-3 px-3 py-1 text-sm text-ink-secondary transition-colors hover:border-stroke-2 hover:bg-fill-4"
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
                  "absolute inset-x-0 bottom-full z-50 flex justify-center",
                  overlayDocked ? "mb-0" : "mb-3",
                  tight ? "px-3" : "px-4",
                )}
              >
                {overlay}
              </div>
            ) : null}
            {composer}
          </div>
        )}

        {composerHero && overlay ? (
          <div
            className={cn(
              "absolute inset-x-0 bottom-6 z-50 flex justify-center",
              tight ? "px-3" : "px-4",
            )}
          >
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

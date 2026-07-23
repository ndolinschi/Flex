import type { CSSProperties, ReactNode } from "react"
import { cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import { Button } from "@/components/ui/button"
import { ChatThreadHeader } from "../molecules/ChatThreadHeader"

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
  /** Empty conversation — utility void; composer stays docked at bottom. */
  composerHero?: boolean
  heroTitle?: string
  heroHint?: string
  /**
   * Production chat thread header title (40px row). Shown when the
   * conversation has turns; empty hero uses the large void title instead.
   */
  threadTitle?: string
  /** Optional trailing controls for the thread header (repo chip, etc.). */
  threadTrailing?: ReactNode
}

/**
 * Chat column shell. Composer always docks at the bottom (IDE rail), even
 * when the conversation is empty — never a centered marketing hero.
 *
 * Empty composition: muted header title + whisper chips in the upper void;
 * the primary control is the docked input below.
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
  heroHint = "Describe a task to get started.",
  threadTitle,
  threadTrailing,
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

  /* Explicit empty string = hide title (tab already names the session);
   * undefined falls back to heroTitle for callers that omit the prop. */
  const headerTitle =
    threadTitle !== undefined ? threadTitle : composerHero ? heroTitle : ""
  const showThreadHeader =
    headerTitle.trim().length > 0 || threadTrailing != null

  const paneStyle = {
    ...(tight ? ({ "--content-rail": "100%" } as CSSProperties) : {}),
  } as CSSProperties

  const pane = (
    <div
      className={cn(
        // Glass chat surface = chrome (#141414 pure gray), not elevated chip.
        "flex h-full min-h-0 min-w-0 flex-1 flex-col bg-chrome",
        wide && "min-w-[380px]",
      )}
      style={Object.keys(paneStyle).length > 0 ? paneStyle : undefined}
    >
      <main className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
        {showThreadHeader ? (
          <ChatThreadHeader title={headerTitle} trailing={threadTrailing} />
        ) : null}
        <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
          <div
            className={cn(
              "flex min-h-0 flex-1 flex-col overflow-hidden",
              composerHero &&
                "pointer-events-none invisible absolute inset-0",
            )}
            aria-hidden={composerHero || undefined}
          >
            {timeline}
          </div>
          {composerHero ? (
            <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-3 pt-6">
              <div className="mx-auto w-full max-w-[var(--content-rail)]">
                <p className="text-sm text-ink-muted">{heroHint}</p>
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {QUICKSTART_SUGGESTIONS.map((suggestion) => (
                    <Button
                      key={suggestion}
                      variant="ghost"
                      size="xs"
                      onClick={() => handleQuickstart(suggestion)}
                      className="h-7 rounded-md border border-stroke-3/80 bg-transparent px-2.5 text-xs font-normal text-ink-muted hover:border-stroke-2 hover:bg-fill-5 hover:text-ink-secondary"
                    >
                      {suggestion}
                    </Button>
                  ))}
                </div>
              </div>
            </div>
          ) : null}
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

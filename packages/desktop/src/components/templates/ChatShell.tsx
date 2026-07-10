import { useEffect, useRef, type ReactNode } from "react"
import { AppHeader } from "../organisms"
import { cn } from "../../lib/utils"

type ChatShellProps = {
  sidebar?: ReactNode
  hideSidebar?: boolean
  timeline: ReactNode
  composer: ReactNode
  overlay?: ReactNode
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
  composerHero = false,
  heroTitle = "Agent",
  heroHint = "Describe a task to start the native agent loop.",
}: ChatShellProps) => {
  const backdropRef = useRef<HTMLDivElement>(null)

  // #region agent log
  useEffect(() => {
    if (!overlay) return
    const backdrop = backdropRef.current
    if (!backdrop) return
    const br = backdrop.getBoundingClientRect()
    fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Debug-Session-Id": "34bae6",
      },
      body: JSON.stringify({
        sessionId: "34bae6",
        runId: "post-fix",
        hypothesisId: "H2",
        location: "ChatShell.tsx:backdrop",
        message: "full-main backdrop geometry",
        data: {
          backdropTop: Math.round(br.top),
          backdropHeight: Math.round(br.height),
          viewportH: window.innerHeight,
          coversFullMain: br.top <= 80 && br.height >= window.innerHeight * 0.7,
          strategy: "absolute-inset-0-on-main",
        },
        timestamp: Date.now(),
      }),
    }).catch(() => {})
  }, [overlay, composerHero])
  // #endregion

  const pane = (
    <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col">
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
            <div className="mx-auto mb-5 w-full max-w-[var(--content-rail)] px-4 text-center">
              <h2 className="mb-4 truncate text-[28px] font-semibold leading-none tracking-[-0.04em] text-ink">
                {heroTitle}
              </h2>
              <p className="text-base text-ink-muted">{heroHint}</p>
            </div>
            {composer}
            <div className="pb-3" />
          </div>
        ) : (
          <div className="relative z-50 shrink-0 pb-3">
            {overlay ? (
              <div className="absolute inset-x-0 bottom-full z-50 mb-3 flex justify-center px-4">
                {overlay}
              </div>
            ) : null}
            {composer}
          </div>
        )}

        {overlay ? (
          <div
            ref={backdropRef}
            className="absolute inset-0 z-40 bg-black/15 animate-backdrop-in"
            aria-hidden
          />
        ) : null}

        {composerHero && overlay ? (
          <div className="absolute inset-x-0 bottom-6 z-50 flex justify-center px-4">
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

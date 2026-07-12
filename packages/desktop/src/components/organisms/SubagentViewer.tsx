import { useEffect } from "react"
import { Bot, X } from "lucide-react"
import { TurnTimeline } from "./TurnTimeline"
import { useAppStore } from "../../stores/appStore"

/** Bottom-anchored overlay showing a subagent's inner session feed, readable
 * while it runs (reference design: tray slides up over the dimmed
 * conversation; outside click / Esc / × closes). Subagent children are real
 * sessions, so the body is the same timeline component as the main chat —
 * replay + live subscribe come for free, just without a composer. */
export const SubagentViewer = () => {
  const viewer = useAppStore((s) => s.subagentViewer)
  const closeSubagentViewer = useAppStore((s) => s.closeSubagentViewer)

  useEffect(() => {
    if (!viewer) return
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation()
        closeSubagentViewer()
      }
    }
    window.addEventListener("keydown", onKeyDown, true)
    return () => window.removeEventListener("keydown", onKeyDown, true)
  }, [viewer, closeSubagentViewer])

  if (!viewer) return null

  return (
    <>
      {/* Dim + click-capture over the conversation behind the tray. */}
      <button
        type="button"
        aria-label="Close subagent view"
        onClick={closeSubagentViewer}
        className="subagent-viewer-mask absolute inset-0 z-10 cursor-default bg-bg/45"
      />
      <section
        role="dialog"
        aria-label={`Subagent: ${viewer.title}`}
        className={[
          "absolute inset-x-0 bottom-0 z-20 flex max-h-[70dvh] min-h-[220px] flex-col overflow-hidden",
          "h-[70dvh] rounded-t-lg border-t border-stroke-3 bg-panel",
          "shadow-[var(--shadow-popover)] animate-subagent-viewer-in",
        ].join(" ")}
      >
        <header className="flex h-9 shrink-0 items-center gap-2 border-b border-stroke-3 px-3">
          <Bot className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
          <span className="min-w-0 flex-1 truncate text-sm text-ink-secondary">
            {viewer.title}
          </span>
          <button
            type="button"
            onClick={closeSubagentViewer}
            aria-label="Close"
            className="rounded-md p-1 text-icon-3 transition-colors duration-[var(--duration-fast)] hover:bg-fill-4 hover:text-ink-secondary"
          >
            <X className="h-3.5 w-3.5" aria-hidden />
          </button>
        </header>
        <div className="flex min-h-0 flex-1 flex-col">
          <TurnTimeline sessionId={viewer.sessionId} />
        </div>
      </section>
    </>
  )
}

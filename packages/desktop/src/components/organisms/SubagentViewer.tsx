import { useState } from "react"
import { Bot, X } from "@/components/icons"
import {
  Drawer,
  DrawerClose,
  DrawerContent,
  DrawerTitle,
} from "@/components/ui/drawer"
import { TurnTimeline } from "./TurnTimeline"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"

/** Bottom-anchored overlay showing a subagent's inner session feed, readable
 * while it runs (vaul Drawer over the dimmed conversation; outside click /
 * Esc / × closes). Subagent children are real sessions, so the body is the
 * same timeline component as the main chat — replay + live subscribe come
 * for free, just without a composer. */
export const SubagentViewer = () => {
  const viewer = useAppStore((s) => s.subagentViewer)
  const closeSubagentViewer = useAppStore((s) => s.closeSubagentViewer)
  const [container, setContainer] = useState<HTMLDivElement | null>(null)

  return (
    <div
      ref={setContainer}
      className="pointer-events-none absolute inset-0 z-20"
    >
      {container ? (
        <Drawer
          open={!!viewer}
          onOpenChange={(open) => {
            if (!open) closeSubagentViewer()
          }}
          direction="bottom"
          container={container}
        >
          <DrawerContent
            aria-label={viewer ? `Subagent: ${viewer.title}` : "Subagent"}
            overlayClassName="absolute bg-bg/45"
            className={cn(
              "pointer-events-auto absolute inset-x-0 bottom-0 flex max-h-[70dvh] min-h-[220px] flex-col overflow-hidden",
              "h-[70dvh] rounded-t-lg border-t border-stroke-3 bg-panel shadow-[var(--shadow-popover)]",
              "data-[vaul-drawer-direction=bottom]:mt-0 data-[vaul-drawer-direction=bottom]:max-h-[70dvh] data-[vaul-drawer-direction=bottom]:rounded-t-lg",
              "[&>div:first-child]:hidden",
            )}
          >
            <header className="flex h-9 shrink-0 items-center gap-2 border-b border-stroke-3 px-3">
              <Bot className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
              <DrawerTitle className="min-w-0 flex-1 truncate text-left text-sm font-normal text-ink-secondary">
                {viewer?.title ?? "Subagent"}
              </DrawerTitle>
              <DrawerClose
                type="button"
                aria-label="Close"
                className="rounded-md p-1 text-icon-3 transition-colors duration-[var(--duration-fast)] hover:bg-fill-4 hover:text-ink-secondary"
              >
                <X className="h-3.5 w-3.5" aria-hidden />
              </DrawerClose>
            </header>
            <div className="flex min-h-0 flex-1 flex-col">
              {viewer ? <TurnTimeline sessionId={viewer.sessionId} /> : null}
            </div>
          </DrawerContent>
        </Drawer>
      ) : null}
    </div>
  )
}

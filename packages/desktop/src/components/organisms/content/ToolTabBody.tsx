import { lazy, Suspense, useMemo } from "react"
import type { SessionMeta } from "../../../lib/types"
import type { RightPanelTab } from "../../../stores/appStore"
import { findPluginTab } from "../../../plugins/registry"
import { PlanTab } from "../right-panel/PlanTab"
import { ChangesTab } from "../right-panel/ChangesTab"
import { MemoryTab } from "../right-panel/MemoryTab"
import { PrTab } from "../right-panel/PrTab"
import { PromptTab } from "../right-panel/PromptTab"
import { StatusTab } from "../right-panel/StatusTab"
import { Spinner } from "../../atoms"
import { cn } from "../../../lib/utils"

/** Heavy panels — keep out of the initial JS graph (Monaco ~3MB, xterm, browser). */
const FilesTab = lazy(() =>
  import("../right-panel/FilesTab").then((m) => ({ default: m.FilesTab })),
)
const TerminalTab = lazy(() =>
  import("../TerminalTab").then((m) => ({ default: m.TerminalTab })),
)
const BrowserTab = lazy(() =>
  import("../BrowserTab").then((m) => ({ default: m.BrowserTab })),
)

type ToolTabBodyProps = {
  tool: RightPanelTab
  session: SessionMeta | undefined
  /** Whether this tab is the active one in its pane (visibility). */
  active: boolean
  /** Keep Files/Terminal/Browser/Prompt mounted while the tab exists in any pane. */
  keepAlive: boolean
}

const PanelFallback = () => (
  <div className="flex h-full items-center justify-center gap-2 text-sm text-ink-muted">
    <Spinner size="sm" />
    Loading…
  </div>
)

/** Renders one tool surface; keep-alive hosts stay mounted when inactive. */
export const ToolTabBody = ({
  tool,
  session,
  active,
  keepAlive,
}: ToolTabBodyProps) => {
  const pluginTab = useMemo(() => findPluginTab(tool), [tool])

  if (tool === "status") {
    if (!session) return null
    return active ? (
      <div className="absolute inset-0 flex flex-col">
        <StatusTab session={session} active={active} />
      </div>
    ) : null
  }

  if (tool === "prompt") {
    if (!session) return null
    return (
      <div
        className={cn(
          "absolute inset-0 flex flex-col",
          active || keepAlive ? (active ? "flex" : "hidden") : "hidden",
        )}
      >
        <PromptTab sessionId={session.id} active={active} />
      </div>
    )
  }

  if (tool === "plan") {
    return active ? (
      <div className="absolute inset-0 flex flex-col">
        <PlanTab active={session} />
      </div>
    ) : null
  }
  if (tool === "changes") {
    return active ? (
      <div className="absolute inset-0 flex flex-col">
        <ChangesTab active={session} />
      </div>
    ) : null
  }
  if (tool === "pr") {
    return active ? (
      <div className="absolute inset-0 flex flex-col">
        <PrTab active={session} />
      </div>
    ) : null
  }
  if (tool === "memory") {
    return active ? (
      <div className="absolute inset-0 flex flex-col">
        <MemoryTab />
      </div>
    ) : null
  }

  if (tool === "files") {
    return (
      <div
        className={cn(
          "absolute inset-0 flex flex-col",
          active || keepAlive ? (active ? "flex" : "hidden") : "hidden",
        )}
      >
        <Suspense fallback={<PanelFallback />}>
          <FilesTab active={active} session={session} />
        </Suspense>
      </div>
    )
  }

  if (tool === "terminal") {
    return (
      <div
        className={cn(
          "absolute inset-0 flex flex-col",
          active || keepAlive ? (active ? "flex" : "hidden") : "hidden",
        )}
      >
        <Suspense fallback={<PanelFallback />}>
          <TerminalTab active={active} sessionId={session?.id ?? null} />
        </Suspense>
      </div>
    )
  }

  if (tool === "browser") {
    return (
      <div
        className={cn(
          "absolute inset-0",
          active || keepAlive ? (active ? "block" : "hidden") : "hidden",
        )}
      >
        <Suspense fallback={<PanelFallback />}>
          <BrowserTab active={active} sessionId={session?.id ?? null} />
        </Suspense>
      </div>
    )
  }

  if (pluginTab) {
    return active ? (
      <div className="absolute inset-0 flex flex-col">
        {pluginTab.render({ active, session })}
      </div>
    ) : null
  }

  return null
}

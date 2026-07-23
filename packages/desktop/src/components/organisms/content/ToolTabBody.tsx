import { lazy, Suspense, useMemo } from "react"
import type { SessionMeta } from "../../../lib/types"
import type { RightPanelTab } from "../../../stores/appStore"
import { findPluginTab } from "../../../plugins/registry"
import { PlanTab } from "../right-panel/PlanTab"
import { ChangesTab } from "../right-panel/ChangesTab"
import { MemoryTab } from "../right-panel/MemoryTab"
import { PrTab } from "../right-panel/PrTab"
import { PromptTab } from "../right-panel/PromptTab"
import { Spinner } from "../../atoms"
import { cn } from "../../../lib/utils"

const FilesTab = lazy(() =>
  import("../right-panel/FilesTab").then((m) => ({ default: m.FilesTab })),
)
const TerminalTab = lazy(() =>
  import("../TerminalTab").then((m) => ({ default: m.TerminalTab })),
)
const BrowserTab = lazy(() =>
  import("../BrowserTab").then((m) => ({ default: m.BrowserTab })),
)
const StatusTab = lazy(() =>
  import("../right-panel/StatusTab").then((m) => ({ default: m.StatusTab })),
)

type ToolTabBodyProps = {
  tool: RightPanelTab
  session: SessionMeta | undefined
  active: boolean
  keepAlive: boolean
}

const PanelFallback = () => (
  <div className="flex h-full items-center justify-center gap-2 text-sm text-ink-muted">
    <Spinner size="sm" />
    Loading…
  </div>
)

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
        <Suspense fallback={<PanelFallback />}>
          <StatusTab session={session} active={active} />
        </Suspense>
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
    return (
      <div
        className={cn(
          "absolute inset-0 flex flex-col",
          active ? "flex" : "hidden",
        )}
      >
        {pluginTab.render({ active, session })}
      </div>
    )
  }

  return null
}

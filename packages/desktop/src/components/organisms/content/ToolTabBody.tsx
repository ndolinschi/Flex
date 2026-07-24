import { lazy, Suspense, useMemo, type ReactNode } from "react"
import type { SessionMeta } from "../../../lib/types"
import type { RightPanelTab } from "../../../stores/appStore"
import { findPluginTab } from "../../../plugins/registry"
import { PlanTab } from "../right-panel/PlanTab"
import { ChangesTab } from "../right-panel/ChangesTab"
import { MemoryTab } from "../right-panel/MemoryTab"
import { PrTab } from "../right-panel/PrTab"
import { PromptTab } from "../right-panel/PromptTab"
import { Spinner } from "../../atoms"
import { PanelErrorBoundary } from "../../templates"
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

const toolLabel = (tool: RightPanelTab, pluginLabel?: string): string => {
  if (pluginLabel) return pluginLabel
  switch (tool) {
    case "status":
      return "Status"
    case "prompt":
      return "Prompt"
    case "plan":
      return "Plan"
    case "changes":
      return "Changes"
    case "pr":
      return "Pull Request"
    case "memory":
      return "Memory"
    case "files":
      return "Files"
    case "terminal":
      return "Terminal"
    case "browser":
      return "Browser"
    case "database":
      return "Database"
    default:
      return tool.charAt(0).toUpperCase() + tool.slice(1)
  }
}

const Guarded = ({
  label,
  children,
}: {
  label: string
  children: ReactNode
}) => (
  <PanelErrorBoundary label={label}>
    {children}
  </PanelErrorBoundary>
)

export const ToolTabBody = ({
  tool,
  session,
  active,
  keepAlive,
}: ToolTabBodyProps) => {
  const pluginTab = useMemo(() => findPluginTab(tool), [tool])
  const label = toolLabel(tool, pluginTab?.label)

  if (tool === "status") {
    if (!session) return null
    return active ? (
      <div className="absolute inset-0 flex flex-col">
        <Guarded label={label}>
          <Suspense fallback={<PanelFallback />}>
            <StatusTab session={session} active={active} />
          </Suspense>
        </Guarded>
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
        <Guarded label={label}>
          <PromptTab sessionId={session.id} active={active} />
        </Guarded>
      </div>
    )
  }

  if (tool === "plan") {
    return active ? (
      <div className="absolute inset-0 flex flex-col">
        <Guarded label={label}>
          <PlanTab active={session} />
        </Guarded>
      </div>
    ) : null
  }
  if (tool === "changes") {
    return active ? (
      <div className="absolute inset-0 flex flex-col">
        <Guarded label={label}>
          <ChangesTab active={session} />
        </Guarded>
      </div>
    ) : null
  }
  if (tool === "pr") {
    return active ? (
      <div className="absolute inset-0 flex flex-col">
        <Guarded label={label}>
          <PrTab active={session} />
        </Guarded>
      </div>
    ) : null
  }
  if (tool === "memory") {
    return active ? (
      <div className="absolute inset-0 flex flex-col">
        <Guarded label={label}>
          <MemoryTab />
        </Guarded>
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
        <Guarded label={label}>
          <Suspense fallback={<PanelFallback />}>
            <FilesTab active={active} session={session} />
          </Suspense>
        </Guarded>
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
        <Guarded label={label}>
          <Suspense fallback={<PanelFallback />}>
            <TerminalTab active={active} sessionId={session?.id ?? null} />
          </Suspense>
        </Guarded>
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
        <Guarded label={label}>
          <Suspense fallback={<PanelFallback />}>
            <BrowserTab active={active} sessionId={session?.id ?? null} />
          </Suspense>
        </Guarded>
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
        <Guarded label={label}>
          {pluginTab.render({ active, session })}
        </Guarded>
      </div>
    )
  }

  return null
}

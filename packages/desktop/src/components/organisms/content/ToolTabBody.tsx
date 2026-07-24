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

/** Keep body mounted (CSS-hidden) when active or keepAlive — avoids remount flicker. */
const bodyClass = (
  active: boolean,
  keepAlive: boolean,
  display: "flex" | "block" = "flex",
): string =>
  cn(
    "absolute inset-0",
    display === "flex" && "flex flex-col",
    active || keepAlive ? (active ? display : "hidden") : "hidden",
  )

export const ToolTabBody = ({
  tool,
  session,
  active,
  keepAlive,
}: ToolTabBodyProps) => {
  const pluginTab = useMemo(() => findPluginTab(tool), [tool])
  const label = toolLabel(tool, pluginTab?.label)
  // Once shown, stay mounted for the life of this tab row (plus keepAlive tools).
  const mounted = active || keepAlive

  if (tool === "status") {
    if (!session || !mounted) return null
    return (
      <div className={bodyClass(active, true)}>
        <Guarded label={label}>
          <Suspense fallback={<PanelFallback />}>
            <StatusTab session={session} active={active} />
          </Suspense>
        </Guarded>
      </div>
    )
  }

  if (tool === "prompt") {
    if (!session || !mounted) return null
    return (
      <div className={bodyClass(active, true)}>
        <Guarded label={label}>
          <PromptTab sessionId={session.id} active={active} />
        </Guarded>
      </div>
    )
  }

  if (tool === "plan") {
    if (!mounted) return null
    return (
      <div className={bodyClass(active, true)}>
        <Guarded label={label}>
          <PlanTab active={session} />
        </Guarded>
      </div>
    )
  }
  if (tool === "changes") {
    if (!mounted) return null
    return (
      <div className={bodyClass(active, true)}>
        <Guarded label={label}>
          <ChangesTab active={session} />
        </Guarded>
      </div>
    )
  }
  if (tool === "pr") {
    if (!mounted) return null
    return (
      <div className={bodyClass(active, true)}>
        <Guarded label={label}>
          <PrTab active={session} />
        </Guarded>
      </div>
    )
  }
  if (tool === "memory") {
    if (!mounted) return null
    return (
      <div className={bodyClass(active, true)}>
        <Guarded label={label}>
          <MemoryTab />
        </Guarded>
      </div>
    )
  }

  if (tool === "files") {
    if (!mounted) return null
    return (
      <div className={bodyClass(active, true)}>
        <Guarded label={label}>
          <Suspense fallback={<PanelFallback />}>
            <FilesTab active={active} session={session} />
          </Suspense>
        </Guarded>
      </div>
    )
  }

  if (tool === "terminal") {
    if (!mounted) return null
    return (
      <div className={bodyClass(active, true)}>
        <Guarded label={label}>
          <Suspense fallback={<PanelFallback />}>
            <TerminalTab active={active} sessionId={session?.id ?? null} />
          </Suspense>
        </Guarded>
      </div>
    )
  }

  if (tool === "browser") {
    if (!mounted) return null
    return (
      <div className={bodyClass(active, true, "block")}>
        <Guarded label={label}>
          <Suspense fallback={<PanelFallback />}>
            <BrowserTab active={active} sessionId={session?.id ?? null} />
          </Suspense>
        </Guarded>
      </div>
    )
  }

  if (pluginTab) {
    if (!mounted) return null
    return (
      <div className={bodyClass(active, true)}>
        <Guarded label={label}>
          {pluginTab.render({ active, session })}
        </Guarded>
      </div>
    )
  }

  return null
}

import { lazy, Suspense, type ReactNode } from "react"
import { Boxes, Database, Package } from "lucide-react"
import { searchDatabaseMentions } from "./database/mentions"
import { searchMcpMentions } from "./mcp/mentions"
import { registerUiPlugin } from "./registry"
import {
  ARTIFACTS_TAB_ENABLED,
  COMPONENTS_TAB_ENABLED,
  DATABASE_TAB_ENABLED,
  INLINE_COMPLETION_ENABLED,
} from "../lib/featureFlags"

/** Plugin tool tabs — lazy so the chat-shell graph does not pay for them
 * until the user opens the tab (mirrors Files/Terminal/Browser). */
const DatabaseTab = lazy(() =>
  import("./database/DatabaseTab").then((m) => ({ default: m.DatabaseTab })),
)
const ComponentsTab = lazy(() =>
  import("./components/ComponentsTab").then((m) => ({
    default: m.ComponentsTab,
  })),
)
const ArtifactsTab = lazy(() =>
  import("./artifacts/ArtifactsTab").then((m) => ({ default: m.ArtifactsTab })),
)

const LazyPluginTab = ({ children }: { children: ReactNode }) => (
  <Suspense
    fallback={
      <div className="flex h-full items-center justify-center text-sm text-ink-muted">
        Loading…
      </div>
    }
  >
    {children}
  </Suspense>
)

/** Register built-in UI plugins. Safe to call once at app boot. */
export const registerBuiltinUiPlugins = (): void => {
  registerUiPlugin({
    id: "database",
    tabs: [
      {
        id: "database",
        label: "Database",
        icon: Database,
        enabled: DATABASE_TAB_ENABLED,
        render: ({ active, session }) => (
          <LazyPluginTab>
            <DatabaseTab active={active} session={session} />
          </LazyPluginTab>
        ),
      },
    ],
    mentionProviders: [
      {
        id: "database-tables",
        search: searchDatabaseMentions,
      },
    ],
  })

  registerUiPlugin({
    id: "components",
    tabs: [
      {
        id: "components",
        label: "Components",
        icon: Boxes,
        enabled: COMPONENTS_TAB_ENABLED,
        render: ({ active, session }) => (
          <LazyPluginTab>
            <ComponentsTab active={active} session={session} />
          </LazyPluginTab>
        ),
      },
    ],
  })

  registerUiPlugin({
    id: "artifacts",
    tabs: [
      {
        id: "artifacts",
        label: "Artifacts",
        icon: Package,
        enabled: ARTIFACTS_TAB_ENABLED,
        render: ({ active, session }) => (
          <LazyPluginTab>
            <ArtifactsTab active={active} session={session} />
          </LazyPluginTab>
        ),
      },
    ],
  })

  // MCP servers as @-mentions (no tab — Settings owns MCP config).
  registerUiPlugin({
    id: "mcp",
    mentionProviders: [
      {
        id: "mcp-servers",
        search: searchMcpMentions,
      },
    ],
  })

  if (INLINE_COMPLETION_ENABLED) {
    registerUiPlugin({
      id: "prompt-completion",
      inlineCompletion: true,
    })
  }
}

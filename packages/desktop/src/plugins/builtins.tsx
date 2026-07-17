import { Boxes, Database } from "@/components/icons"
import { DatabaseTab } from "./database/DatabaseTab"
import { ComponentsTab } from "./components/ComponentsTab"
import { searchDatabaseMentions } from "./database/mentions"
import { searchMcpMentions } from "./mcp/mentions"
import { registerUiPlugin } from "./registry"
import {
  COMPONENTS_TAB_ENABLED,
  DATABASE_TAB_ENABLED,
  INLINE_COMPLETION_ENABLED,
} from "../lib/featureFlags"

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
          <DatabaseTab active={active} session={session} />
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
          <ComponentsTab active={active} session={session} />
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

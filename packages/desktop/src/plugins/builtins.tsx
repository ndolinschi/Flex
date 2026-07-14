import { Database } from "lucide-react"
import { DatabaseTab } from "./database/DatabaseTab"
import { searchDatabaseMentions } from "./database/mentions"
import { registerUiPlugin } from "./registry"
import { DATABASE_TAB_ENABLED } from "../lib/featureFlags"

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
}

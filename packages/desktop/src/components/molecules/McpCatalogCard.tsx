import { Badge, Button } from "../atoms"
import type { McpCatalogEntry } from "../../lib/mcpCatalog"

type McpCatalogCardProps = {
  entry: McpCatalogEntry
  installed: boolean
  installing: boolean
  onInstall: (entry: McpCatalogEntry) => void
}

/** Compact catalog row for the "Browse catalog" list — name + one-line
 * truncated description on the left, right-aligned Install button /
 * Installed badge, matching `SettingsSection`'s inset-divider row anatomy
 * so the catalog reads as one family with the "MCP servers" list below it. */
export const McpCatalogCard = ({
  entry,
  installed,
  installing,
  onInstall,
}: McpCatalogCardProps) => {
  return (
    <div className="flex items-center gap-3 px-3.5 py-3">
      <div className="min-w-0 flex-1">
        <p className="truncate text-[13px] text-ink-secondary">{entry.name}</p>
        <p className="mt-0.5 truncate text-[13px] text-ink-muted">{entry.description}</p>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {installed ? (
          <Badge variant="success">Installed</Badge>
        ) : (
          <Button
            variant="secondary"
            size="sm"
            isLoading={installing}
            disabled={installing}
            onClick={() => onInstall(entry)}
          >
            Install
          </Button>
        )}
      </div>
    </div>
  )
}

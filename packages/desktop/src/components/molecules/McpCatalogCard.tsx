import { Badge } from "../atoms"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import type { McpCatalogEntry } from "../../lib/mcpCatalog"
import { catalogEntryNeedsConfig } from "../../lib/mcpCatalog"

type McpCatalogCardProps = {
  entry: McpCatalogEntry
  installed: boolean
  installing: boolean
  onInstall: (entry: McpCatalogEntry) => void
  onConfigure?: (entry: McpCatalogEntry) => void
}

export const McpCatalogCard = ({
  entry,
  installed,
  installing,
  onInstall,
  onConfigure,
}: McpCatalogCardProps) => {
  const needsConfig = catalogEntryNeedsConfig(entry)

  return (
    <div className="flex items-center gap-3 px-3.5 py-3">
      <div className="min-w-0 flex-1">
        <p className="truncate text-base text-ink-secondary">{entry.name}</p>
        <p className="mt-0.5 truncate text-base text-ink-muted">{entry.description}</p>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {installed ? (
          <>
            <Badge variant="success">Installed</Badge>
            {needsConfig && onConfigure ? (
              <Button
                variant="secondary"
                size="sm"
                disabled={installing}
                onClick={() => onConfigure(entry)}
              >
                Configure
              </Button>
            ) : null}
          </>
        ) : (
          <Button
            variant="secondary"
            size="sm"
            disabled={installing}
            onClick={() => onInstall(entry)}
          >
            {installing ? <Spinner data-icon="inline-start" /> : null}
            Install
          </Button>
        )}
      </div>
    </div>
  )
}

import { useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { Plus } from "@/components/icons"
import { Button, Spinner } from "../../../components/atoms"
import { ErrorBanner, SettingsSection } from "../../../components/molecules"
import { mcpList, toInvokeError } from "../../../lib/tauri"
import { CreateMcpServerForm } from "./CreateMcpServerForm"
import { McpServerRow } from "./McpServerRow"
import { EMPTY_MCP_SERVERS, MCP_SERVERS_KEY } from "./mcpServersKey"

/** MCP (Model Context Protocol) server management — stdio servers whose
 * tools get bridged into the tool registry on the next service rebuild.
 * Structured like AutomationsPage (list + inline add-form + per-row
 * delete-confirm) rather than the fixed plugin-toggle catalog above, since
 * servers are user-added CRUD entries with distinct identities. */
export const McpServersSection = () => {
  const [creating, setCreating] = useState(false)
  const queryClient = useQueryClient()

  const serversQuery = useQuery({
    queryKey: MCP_SERVERS_KEY,
    queryFn: mcpList,
  })

  const servers = serversQuery.data ?? EMPTY_MCP_SERVERS

  return (
    <div className="flex flex-col gap-3">
      <SettingsSection
        title="MCP servers"
        description="Tools from stdio MCP servers. Restart sessions to pick up new or changed servers."
        rowId="tools-mcp-servers"
        className="mb-0"
        actions={
          !creating ? (
            <Button size="sm" onClick={() => setCreating(true)}>
              <Plus className="h-3.5 w-3.5" aria-hidden /> Add server
            </Button>
          ) : undefined
        }
      >
        {serversQuery.isLoading ? (
          <div className="flex items-center gap-2 px-3.5 py-3 text-sm text-ink-muted">
            <Spinner size="sm" /> Loading MCP servers…
          </div>
        ) : serversQuery.isError ? (
          <div className="px-3.5 py-3">
            <ErrorBanner message={toInvokeError(serversQuery.error)} />
          </div>
        ) : servers.length === 0 ? (
          <p className="px-3.5 py-3 text-sm text-ink-muted">No MCP servers configured yet.</p>
        ) : (
          servers.map((server) => <McpServerRow key={server.id} server={server} />)
        )}
      </SettingsSection>

      {creating ? (
        <CreateMcpServerForm
          onCancel={() => setCreating(false)}
          onSaved={() => {
            setCreating(false)
            void queryClient.invalidateQueries({ queryKey: MCP_SERVERS_KEY })
          }}
        />
      ) : null}
    </div>
  )
}

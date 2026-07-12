import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { TextInput } from "../../../components/atoms"
import { McpCatalogCard, McpInstallDialog, SettingsSection } from "../../../components/molecules"
import { useAppStore } from "../../../stores/appStore"
import { MCP_CATALOG, type McpCatalogEntry } from "../../../lib/mcpCatalog"
import { buildCatalogServerDto } from "../../../lib/mcp"
import { mcpList, mcpUpsert, toInvokeError } from "../../../lib/tauri"
import type { McpServerDto } from "../../../lib/types"
import { EMPTY_MCP_SERVERS, MCP_SERVERS_KEY } from "./mcpServersKey"

/** Curated "Browse catalog" grid of popular MCP servers, additive above the
 * manual add-server flow below. Cards with no required args/env install
 * directly; cards that need them (a path, a token, a connection string)
 * open `McpInstallDialog` first. Installed-state badge is matched against
 * live `mcp_list` results by id, so re-opening Settings after install shows
 * "Installed" even without a page refresh (react-query cache). */
export const McpCatalogSection = () => {
  const queryClient = useQueryClient()
  const pushToast = useAppStore((s) => s.pushToast)
  const [query, setQuery] = useState("")
  const [pendingEntry, setPendingEntry] = useState<McpCatalogEntry | null>(null)
  const [dialogError, setDialogError] = useState<string | null>(null)

  const serversQuery = useQuery({
    queryKey: MCP_SERVERS_KEY,
    queryFn: mcpList,
  })
  const installedIds = useMemo(
    () => new Set((serversQuery.data ?? EMPTY_MCP_SERVERS).map((s) => s.id)),
    [serversQuery.data],
  )

  const installMutation = useMutation({
    mutationFn: (dto: McpServerDto) => mcpUpsert(dto),
    onSuccess: (_data, dto) => {
      void queryClient.invalidateQueries({ queryKey: MCP_SERVERS_KEY })
      pushToast(`${dto.id} installed`, "success")
      setPendingEntry(null)
      setDialogError(null)
    },
    onError: (err) => {
      pushToast(toInvokeError(err), "error")
      setDialogError(toInvokeError(err))
    },
  })

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return MCP_CATALOG
    return MCP_CATALOG.filter(
      (entry) =>
        entry.name.toLowerCase().includes(q) ||
        entry.description.toLowerCase().includes(q) ||
        entry.id.toLowerCase().includes(q),
    )
  }, [query])

  const handleInstall = (entry: McpCatalogEntry) => {
    if (entry.argKeys.length === 0 && entry.envKeys.length === 0) {
      installMutation.mutate(buildCatalogServerDto(entry, { args: {}, env: {} }))
      return
    }
    setDialogError(null)
    setPendingEntry(entry)
  }

  return (
    <>
      <SettingsSection
        title="Browse catalog"
        description="One-click install for popular MCP servers."
        rowId="tools-mcp-catalog"
        actions={
          <TextInput
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search catalog…"
            aria-label="Search MCP catalog"
            className="w-56"
          />
        }
      >
        {visible.length === 0 ? (
          <p className="p-8 text-center text-sm text-ink-muted">
            No catalog servers match “{query}”.
          </p>
        ) : (
          visible.map((entry) => (
            <McpCatalogCard
              key={entry.id}
              entry={entry}
              installed={installedIds.has(entry.id)}
              installing={installMutation.isPending && installMutation.variables?.id === entry.id}
              onInstall={handleInstall}
            />
          ))
        )}
      </SettingsSection>

      <McpInstallDialog
        entry={pendingEntry}
        isLoading={installMutation.isPending}
        error={dialogError}
        onCancel={() => {
          setPendingEntry(null)
          setDialogError(null)
        }}
        onInstall={(values) => {
          if (!pendingEntry) return
          installMutation.mutate(buildCatalogServerDto(pendingEntry, values))
        }}
      />
    </>
  )
}

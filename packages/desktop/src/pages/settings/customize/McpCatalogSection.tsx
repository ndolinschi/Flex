import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { TextInput } from "../../../components/atoms"
import { McpCatalogCard, McpInstallDialog, SettingsSection } from "../../../components/molecules"
import { useAppStore } from "../../../stores/appStore"
import {
  MCP_CATALOG,
  catalogEntryNeedsConfig,
  type McpCatalogEntry,
} from "../../../lib/mcpCatalog"
import { buildCatalogServerDto, prefillCatalogValues } from "../../../lib/mcp"
import { mcpList, mcpUpsert, toInvokeError } from "../../../lib/tauri"
import type { McpServerDto } from "../../../lib/types"
import { EMPTY_MCP_SERVERS, MCP_SERVERS_KEY } from "./mcpServersKey"

/** Curated "Browse catalog" grid of popular MCP servers, additive above the
 * manual add-server flow below. Cards with no required args/env install
 * directly; cards that need them (a path, a token, a connection string)
 * open `McpInstallDialog` first. Installed catalog entries that need config
 * expose Configure (re-opens the dialog; secrets keep-if-blank). */
export const McpCatalogSection = () => {
  const queryClient = useQueryClient()
  const pushToast = useAppStore((s) => s.pushToast)
  const [query, setQuery] = useState("")
  const [pendingEntry, setPendingEntry] = useState<McpCatalogEntry | null>(null)
  const [dialogMode, setDialogMode] = useState<"install" | "configure">("install")
  const [dialogError, setDialogError] = useState<string | null>(null)

  const serversQuery = useQuery({
    queryKey: MCP_SERVERS_KEY,
    queryFn: mcpList,
  })
  const servers = serversQuery.data ?? EMPTY_MCP_SERVERS
  const installedById = useMemo(() => {
    const map = new Map<string, McpServerDto>()
    for (const s of servers) map.set(s.id, s)
    return map
  }, [servers])

  const installMutation = useMutation({
    mutationFn: (dto: McpServerDto) => mcpUpsert(dto),
    onSuccess: (_data, dto) => {
      void queryClient.invalidateQueries({ queryKey: MCP_SERVERS_KEY })
      pushToast(
        dialogMode === "configure" ? `${dto.id} updated` : `${dto.id} installed`,
        "success",
      )
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
    if (!catalogEntryNeedsConfig(entry)) {
      installMutation.mutate(buildCatalogServerDto(entry, { args: {}, env: {} }))
      return
    }
    setDialogMode("install")
    setDialogError(null)
    setPendingEntry(entry)
  }

  const handleConfigure = (entry: McpCatalogEntry) => {
    setDialogMode("configure")
    setDialogError(null)
    setPendingEntry(entry)
  }

  const existing = pendingEntry ? installedById.get(pendingEntry.id) : undefined
  const initialValues =
    pendingEntry && existing && dialogMode === "configure"
      ? prefillCatalogValues(pendingEntry, existing)
      : null

  return (
    <>
      <SettingsSection
        title="Browse catalog"
        description="One-click install for popular MCP servers. Tokens and connection strings are stored in the encrypted secrets store."
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
              installed={installedById.has(entry.id)}
              installing={installMutation.isPending && installMutation.variables?.id === entry.id}
              onInstall={handleInstall}
              onConfigure={handleConfigure}
            />
          ))
        )}
      </SettingsSection>

      <McpInstallDialog
        entry={pendingEntry}
        mode={dialogMode}
        initialValues={initialValues}
        configuredSecretEnv={existing?.configuredSecretEnv ?? []}
        hasSecretArgs={existing?.hasSecretArgs ?? false}
        isLoading={installMutation.isPending}
        error={dialogError}
        onCancel={() => {
          setPendingEntry(null)
          setDialogError(null)
        }}
        onInstall={(values) => {
          if (!pendingEntry) return
          const dto = buildCatalogServerDto(pendingEntry, values)
          if (dialogMode === "configure" && existing) {
            // Preserve enabled flag and only send secret fields that the user
            // typed (empty = keep existing — handled by Rust upsert).
            installMutation.mutate({
              ...dto,
              enabled: existing.enabled,
              secretEnv: Object.fromEntries(
                Object.entries(dto.secretEnv ?? {}).filter(([, v]) => v.trim().length > 0),
              ),
              secretArgs:
                dto.secretArgs && dto.secretArgs.some((v) => v.trim().length > 0)
                  ? dto.secretArgs
                  : undefined,
            })
            return
          }
          installMutation.mutate(dto)
        }}
      />
    </>
  )
}

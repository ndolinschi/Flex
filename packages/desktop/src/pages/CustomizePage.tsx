import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { BookOpen, Check, Globe, Plug, Plus, ShieldCheck, Trash2 } from "lucide-react"
import { Badge, Button, Spinner, TextArea, TextInput } from "../components/atoms"
import {
  ConfirmDialog,
  ErrorBanner,
  FieldRow,
  McpCatalogCard,
  McpInstallDialog,
  SettingsSection,
} from "../components/molecules"
import { useProviderConfig } from "../hooks/useProviderConfig"
import { useAppStore } from "../stores/appStore"
import { MCP_CATALOG, type McpCatalogEntry } from "../lib/mcpCatalog"
import { mcpList, mcpRemove, mcpTest, mcpUpsert, toInvokeError } from "../lib/tauri"
import type { McpServerDto, PluginPrefs } from "../lib/types"
import { cn } from "../lib/utils"

type PluginKey = keyof PluginPrefs

type PluginCardSpec = {
  key: PluginKey
  name: string
  description: string
  icon: typeof Globe
  category: string
}

/** Engine plugin catalog — mirrors the fixed PluginPrefs shape on the wire. */
const PLUGIN_CATALOG: PluginCardSpec[] = [
  {
    key: "search",
    name: "Search",
    description: "Web tools: search_web + scrape_page with a researcher role.",
    icon: Globe,
    category: "Engine plugins",
  },
  {
    key: "learning",
    name: "Learning",
    description: "Persistent memory and skills: SkillSave / MemoryWrite + reflection.",
    icon: BookOpen,
    category: "Engine plugins",
  },
  {
    key: "verifier",
    name: "Verifier",
    description: "Independent grading of results: Verify / SubmitVerdict tools.",
    icon: ShieldCheck,
    category: "Engine plugins",
  },
]

const MCP_SERVERS_KEY = ["mcp-servers"] as const
const EMPTY_MCP_SERVERS: McpServerDto[] = []

type McpFormState = {
  id: string
  command: string
  args: string
  envText: string
  enabled: boolean
}

const emptyMcpForm = (): McpFormState => ({
  id: "",
  command: "",
  args: "",
  envText: "",
  enabled: true,
})

/** Splits on whitespace, dropping empties — good enough for `npx -y pkg` style commands. */
const parseArgs = (raw: string): string[] =>
  raw
    .split(/\s+/)
    .map((s) => s.trim())
    .filter(Boolean)

/** One `KEY=value` pair per line; blank lines and lines without `=` are ignored. */
const parseEnv = (raw: string): Record<string, string> => {
  const env: Record<string, string> = {}
  for (const line of raw.split("\n")) {
    const trimmed = line.trim()
    if (!trimmed) continue
    const idx = trimmed.indexOf("=")
    if (idx <= 0) continue
    env[trimmed.slice(0, idx).trim()] = trimmed.slice(idx + 1).trim()
  }
  return env
}

const MCP_ID_RE = /^[a-z0-9]+(-[a-z0-9]+)*$/

/** Inline create form for a new MCP server — stdio transport only (MVP). */
const CreateMcpServerForm = ({
  onCancel,
  onSaved,
}: {
  onCancel: () => void
  onSaved: () => void
}) => {
  const [form, setForm] = useState<McpFormState>(emptyMcpForm())
  const [error, setError] = useState<string | null>(null)

  const upsertMutation = useMutation({
    mutationFn: (server: McpServerDto) => mcpUpsert(server),
  })

  const patch = (partial: Partial<McpFormState>) =>
    setForm((prev) => ({ ...prev, ...partial }))

  const handleSave = async () => {
    setError(null)
    const id = form.id.trim()
    if (!id) {
      setError("Id is required")
      return
    }
    if (!MCP_ID_RE.test(id)) {
      setError("Id must be kebab-case (lowercase letters, numbers, hyphens)")
      return
    }
    if (!form.command.trim()) {
      setError("Command is required")
      return
    }

    const server: McpServerDto = {
      id,
      command: form.command.trim(),
      args: parseArgs(form.args),
      env: parseEnv(form.envText),
      enabled: form.enabled,
    }

    try {
      await upsertMutation.mutateAsync(server)
      onSaved()
    } catch (err) {
      setError(toInvokeError(err))
    }
  }

  return (
    <SettingsSection title="New MCP server">
      <FieldRow
        label="Id"
        htmlFor="mcp-id"
        hint={`Kebab-case, e.g. "filesystem" — tools appear as "${form.id || "id"}__<tool>".`}
      >
        <TextInput
          id="mcp-id"
          value={form.id}
          onChange={(e) => patch({ id: e.target.value })}
          placeholder="filesystem"
        />
      </FieldRow>

      <FieldRow label="Command" htmlFor="mcp-command">
        <TextInput
          id="mcp-command"
          value={form.command}
          onChange={(e) => patch({ command: e.target.value })}
          placeholder="npx"
        />
      </FieldRow>

      <FieldRow label="Arguments (optional)" htmlFor="mcp-args" hint="Space-separated.">
        <TextInput
          id="mcp-args"
          value={form.args}
          onChange={(e) => patch({ args: e.target.value })}
          placeholder="-y @modelcontextprotocol/server-filesystem /path/to/project"
        />
      </FieldRow>

      <FieldRow
        label="Environment variables (optional)"
        htmlFor="mcp-env"
        hint='One "KEY=value" pair per line.'
      >
        <TextArea
          id="mcp-env"
          value={form.envText}
          onChange={(e) => patch({ envText: e.target.value })}
          placeholder={"API_KEY=...\nOTHER_VAR=..."}
          rows={3}
        />
      </FieldRow>

      <FieldRow label="Enabled" htmlFor="mcp-enabled">
        <input
          id="mcp-enabled"
          type="checkbox"
          checked={form.enabled}
          onChange={(e) => patch({ enabled: e.target.checked })}
          className="h-3.5 w-3.5 rounded border-border accent-accent"
        />
      </FieldRow>

      {error ? (
        <div className="px-4 py-3">
          <ErrorBanner message={error} onDismiss={() => setError(null)} />
        </div>
      ) : null}

      <div className="flex justify-end gap-2 px-4 py-3">
        <Button variant="secondary" size="sm" onClick={onCancel}>
          Cancel
        </Button>
        <Button size="sm" isLoading={upsertMutation.isPending} onClick={() => void handleSave()}>
          Save server
        </Button>
      </div>
    </SettingsSection>
  )
}

const McpServerRow = ({ server }: { server: McpServerDto }) => {
  const queryClient = useQueryClient()
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [testResult, setTestResult] = useState<
    { kind: "ok"; tools: string[] } | { kind: "error"; message: string } | null
  >(null)

  const toggleMutation = useMutation({
    mutationFn: () => mcpUpsert({ ...server, enabled: !server.enabled }),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: MCP_SERVERS_KEY }),
  })

  const removeMutation = useMutation({
    mutationFn: () => mcpRemove(server.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: MCP_SERVERS_KEY })
      setConfirmDelete(false)
    },
  })

  const testMutation = useMutation({
    mutationFn: () => mcpTest(server.id),
    onSuccess: (tools) => setTestResult({ kind: "ok", tools }),
    onError: (err) => setTestResult({ kind: "error", message: toInvokeError(err) }),
  })

  return (
    <div className="flex flex-col">
      <div className="flex items-start gap-3 p-3">
        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-fill-3">
          <Plug className="h-4 w-4 text-icon-2" aria-hidden />
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="truncate text-[13px] text-ink">{server.id}</p>
            <Badge variant={server.enabled ? "success" : "muted"}>
              {server.enabled ? "Enabled" : "Disabled"}
            </Badge>
          </div>
          <p className="mt-0.5 truncate font-mono text-[11px] text-ink-muted">
            {server.command}
            {server.args.length ? ` ${server.args.join(" ")}` : ""}
          </p>
          {testResult?.kind === "ok" ? (
            <p className="mt-1 text-xs text-accent">
              {testResult.tools.length > 0
                ? `Connected — ${testResult.tools.length} tool${testResult.tools.length === 1 ? "" : "s"}: ${testResult.tools.join(", ")}`
                : "Connected — no tools reported."}
            </p>
          ) : testResult?.kind === "error" ? (
            <p className="mt-1 text-xs text-danger">{testResult.message}</p>
          ) : null}
        </div>

        <div className="flex shrink-0 items-center gap-3">
          <label className="flex items-center gap-1.5 text-xs text-ink-secondary">
            <input
              type="checkbox"
              checked={server.enabled}
              disabled={toggleMutation.isPending}
              onChange={() => void toggleMutation.mutateAsync()}
              className="h-3.5 w-3.5 rounded border-border accent-accent"
            />
            Enabled
          </label>
          <Button
            variant="secondary"
            size="sm"
            isLoading={testMutation.isPending}
            onClick={() => {
              setTestResult(null)
              void testMutation.mutateAsync()
            }}
          >
            Test
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="text-danger"
            onClick={() => setConfirmDelete(true)}
          >
            <Trash2 className="h-3 w-3" aria-hidden />
          </Button>
        </div>
      </div>

      <ConfirmDialog
        open={confirmDelete}
        title={`Remove "${server.id}"?`}
        description="This deletes the saved server. Restart sessions to pick up the change."
        confirmLabel="Remove"
        danger
        isLoading={removeMutation.isPending}
        onConfirm={() => void removeMutation.mutateAsync()}
        onCancel={() => setConfirmDelete(false)}
      />
    </div>
  )
}

/** Assembles an `McpServerDto` for a catalog entry from the install
 * dialog's collected values — positional `argKeys` values are appended
 * after the entry's literal `args` (e.g. filesystem's path, postgres's
 * connection string), and `envKeys` values become the `env` map. */
const buildCatalogServerDto = (
  entry: McpCatalogEntry,
  values: { args: Record<string, string>; env: Record<string, string> },
): McpServerDto => ({
  id: entry.id,
  command: entry.command,
  args: [
    ...entry.args,
    ...entry.argKeys.map((a) => values.args[a.key]?.trim() ?? "").filter(Boolean),
  ],
  env: Object.fromEntries(
    entry.envKeys
      .map((e) => [e.name, values.env[e.name]?.trim() ?? ""] as const)
      .filter(([, v]) => v.length > 0),
  ),
  enabled: true,
})

/** Curated "Browse catalog" grid of popular MCP servers, additive above the
 * manual add-server flow below. Cards with no required args/env install
 * directly; cards that need them (a path, a token, a connection string)
 * open `McpInstallDialog` first. Installed-state badge is matched against
 * live `mcp_list` results by id, so re-opening Settings after install shows
 * "Installed" even without a page refresh (react-query cache). */
const McpCatalogSection = () => {
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

/** MCP (Model Context Protocol) server management — stdio servers whose
 * tools get bridged into the tool registry on the next service rebuild.
 * Structured like AutomationsPage (list + inline add-form + per-row
 * delete-confirm) rather than the fixed plugin-toggle catalog above, since
 * servers are user-added CRUD entries with distinct identities. */
const McpServersSection = () => {
  const [creating, setCreating] = useState(false)
  const queryClient = useQueryClient()

  const serversQuery = useQuery({
    queryKey: MCP_SERVERS_KEY,
    queryFn: mcpList,
  })

  const servers = serversQuery.data ?? EMPTY_MCP_SERVERS

  return (
    <div className="flex flex-col gap-4">
      <SettingsSection
        title="MCP servers"
        description="Tools from stdio MCP servers. Restart sessions to pick up new or changed servers."
        rowId="tools-mcp-servers"
        actions={
          !creating ? (
            <Button size="sm" onClick={() => setCreating(true)}>
              <Plus className="h-3.5 w-3.5" aria-hidden /> Add server
            </Button>
          ) : undefined
        }
      >
        {serversQuery.isLoading ? (
          <div className="flex items-center gap-2 p-3 text-sm text-ink-muted">
            <Spinner size="sm" /> Loading MCP servers…
          </div>
        ) : serversQuery.isError ? (
          <div className="p-3">
            <ErrorBanner message={toInvokeError(serversQuery.error)} />
          </div>
        ) : servers.length === 0 ? (
          <p className="p-3 text-sm text-ink-muted">No MCP servers configured yet.</p>
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

/** Customize content: searchable plugin cards with Add / Added, plus the MCP
 * servers list. Mounted inside the Settings shell's "Tools & MCP" section
 * (design-map/07-settings.md build brief §3) — the standalone Customize
 * route/page is gone; `App.tsx` now renders the unified settings shell for
 * all of settings/customize/automations/memory. No `SettingsShell` wrapper
 * here anymore since the shell itself owns nav+header. */
export const CustomizeContent = () => {
  const { config, isLoading, save } = useProviderConfig()
  const [query, setQuery] = useState("")
  const [busyKey, setBusyKey] = useState<PluginKey | null>(null)
  const [error, setError] = useState<string | null>(null)

  const plugins = config?.plugins

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return PLUGIN_CATALOG
    return PLUGIN_CATALOG.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        p.description.toLowerCase().includes(q),
    )
  }, [query])

  const handleToggle = async (key: PluginKey) => {
    if (!config || !plugins || busyKey) return
    setError(null)
    setBusyKey(key)
    try {
      // Round-trip every field: save_provider_config overwrites baseUrl and
      // defaultModel unconditionally, so a plugins-only payload would wipe them.
      await save({
        preferredProvider: config.preferredProvider ?? "",
        baseUrl: config.baseUrl,
        defaultModel: config.defaultModel,
        fallbackModels: config.fallbackModels,
        defaultIsolation:
          typeof config.defaultIsolation === "string"
            ? config.defaultIsolation
            : undefined,
        plugins: { ...plugins, [key]: !plugins[key] },
      })
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusyKey(null)
    }
  }

  const searchInput = (
    <TextInput
      value={query}
      onChange={(e) => setQuery(e.target.value)}
      placeholder="Search plugins…"
      aria-label="Search plugins"
      className="w-56"
    />
  )

  return (
      <div className="flex flex-col gap-4">
        {error ? <ErrorBanner message={error} onDismiss={() => setError(null)} /> : null}

        <SettingsSection
          title="Engine plugins"
          description="Native tool bundles the engine can load into a session."
          rowId="tools-plugins"
          actions={searchInput}
        >
          {isLoading || !plugins ? (
            <div className="flex items-center gap-2 p-3 text-sm text-ink-muted">
              <Spinner size="sm" /> Loading configuration…
            </div>
          ) : visible.length === 0 ? (
            <p className="p-8 text-center text-sm text-ink-muted">
              No plugins match “{query}”.
            </p>
          ) : (
            <div className="p-3">
              <div className="grid grid-cols-1 items-stretch gap-3 sm:grid-cols-2">
                {visible.map((plugin) => {
                  const Icon = plugin.icon
                  const added = plugins[plugin.key]
                  const busy = busyKey === plugin.key
                  return (
                    <div
                      key={plugin.key}
                      className="relative flex min-h-[112px] flex-col gap-2 rounded-lg border border-stroke-3 bg-panel p-3"
                    >
                      <div className="flex items-start gap-3">
                        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-fill-3">
                          <Icon className="h-4 w-4 text-icon-2" aria-hidden />
                        </div>
                        <div className="min-w-0 flex-1">
                          <p className="text-[13px] text-ink">{plugin.name}</p>
                          <p className="mt-0.5 line-clamp-2 text-xs leading-normal text-ink-muted">
                            {plugin.description}
                          </p>
                        </div>
                      </div>
                      <div className="mt-auto flex justify-end">
                        <Button
                          variant={added ? "ghost" : "secondary"}
                          size="sm"
                          isLoading={busy}
                          disabled={busyKey !== null && !busy}
                          onClick={() => void handleToggle(plugin.key)}
                          className={cn("shrink-0", added && "text-green")}
                        >
                          {added ? (
                            <>
                              <Check className="h-3 w-3" aria-hidden /> Added
                            </>
                          ) : (
                            "Add"
                          )}
                        </Button>
                      </div>
                    </div>
                  )
                })}
              </div>
            </div>
          )}
        </SettingsSection>

        <McpCatalogSection />
        <McpServersSection />
      </div>
  )
}

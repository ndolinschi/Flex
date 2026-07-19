import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Plug, Settings2, Trash2 } from "lucide-react"
import { Badge, Button } from "../../../components/atoms"
import {
  ConfirmDialog,
  ErrorBanner,
  McpInstallDialog,
} from "../../../components/molecules"
import { buildCatalogServerDto, prefillCatalogValues } from "../../../lib/mcp"
import { catalogEntryNeedsConfig, findCatalogEntry } from "../../../lib/mcpCatalog"
import { mcpRemove, mcpTest, mcpUpsert, toInvokeError } from "../../../lib/tauri"
import type { McpServerDto } from "../../../lib/types"
import { MCP_SERVERS_KEY } from "./mcpServersKey"

export const McpServerRow = ({ server }: { server: McpServerDto }) => {
  const queryClient = useQueryClient()
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [configuring, setConfiguring] = useState(false)
  const [configError, setConfigError] = useState<string | null>(null)
  const [testResult, setTestResult] = useState<
    { kind: "ok"; tools: string[] } | { kind: "error"; message: string } | null
  >(null)

  const catalogEntry = findCatalogEntry(server.id)
  const canConfigure = Boolean(catalogEntry && catalogEntryNeedsConfig(catalogEntry))
  const secretCount = server.configuredSecretEnv?.length ?? 0

  const toggleMutation = useMutation({
    mutationFn: () =>
      mcpUpsert({
        id: server.id,
        command: server.command,
        args: server.args,
        env: server.env,
        enabled: !server.enabled,
      }),
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

  const configureMutation = useMutation({
    mutationFn: (dto: McpServerDto) => mcpUpsert(dto),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: MCP_SERVERS_KEY })
      setConfiguring(false)
      setConfigError(null)
    },
    onError: (err) => setConfigError(toInvokeError(err)),
  })

  return (
    <div className="flex flex-col">
      <div className="flex items-start gap-3 px-3.5 py-3">
        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-fill-3">
          <Plug className="h-4 w-4 text-icon-2" aria-hidden />
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="truncate text-base text-ink">{server.id}</p>
            <Badge variant={server.enabled ? "success" : "muted"}>
              {server.enabled ? "Enabled" : "Disabled"}
            </Badge>
            {secretCount > 0 || server.hasSecretArgs ? (
              <Badge variant="muted">
                {secretCount > 0
                  ? `${secretCount} secret${secretCount === 1 ? "" : "s"}`
                  : "Secrets"}
              </Badge>
            ) : null}
          </div>
          <p className="mt-0.5 truncate font-mono text-xs text-ink-muted">
            {server.command}
            {server.args.length ? ` ${server.args.join(" ")}` : ""}
            {server.hasSecretArgs ? " …" : ""}
          </p>
          {testResult?.kind === "ok" ? (
            <p className="mt-1 text-xs text-accent">
              {testResult.tools.length > 0
                ? `Connected — ${testResult.tools.length} tool${testResult.tools.length === 1 ? "" : "s"}: ${testResult.tools.join(", ")}`
                : "Connected — no tools reported."}
            </p>
          ) : testResult?.kind === "error" ? (
            <div className="mt-1">
              <ErrorBanner
                message={testResult.message}
                className="py-1.5 text-xs"
              />
            </div>
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
          {canConfigure ? (
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                setConfigError(null)
                setConfiguring(true)
              }}
              aria-label={`Configure ${server.id}`}
            >
              <Settings2 className="h-3 w-3" aria-hidden />
            </Button>
          ) : null}
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
            variant="destructive"
            size="sm"
            onClick={() => setConfirmDelete(true)}
          >
            <Trash2 className="h-3 w-3" aria-hidden />
          </Button>
        </div>
      </div>

      <ConfirmDialog
        open={confirmDelete}
        title={`Remove "${server.id}"?`}
        description="This deletes the saved server and any stored secrets. Restart sessions to pick up the change."
        confirmLabel="Remove"
        danger
        isLoading={removeMutation.isPending}
        onConfirm={() => void removeMutation.mutateAsync()}
        onCancel={() => setConfirmDelete(false)}
      />

      {canConfigure && catalogEntry && configuring ? (
        <McpInstallDialog
          entry={catalogEntry}
          mode="configure"
          initialValues={prefillCatalogValues(catalogEntry, server)}
          configuredSecretEnv={server.configuredSecretEnv ?? []}
          hasSecretArgs={server.hasSecretArgs ?? false}
          isLoading={configureMutation.isPending}
          error={configError}
          onCancel={() => {
            setConfiguring(false)
            setConfigError(null)
          }}
          onInstall={(values) => {
            const dto = buildCatalogServerDto(catalogEntry, values)
            configureMutation.mutate({
              ...dto,
              enabled: server.enabled,
              secretEnv: Object.fromEntries(
                Object.entries(dto.secretEnv ?? {}).filter(([, v]) => v.trim().length > 0),
              ),
              secretArgs:
                dto.secretArgs && dto.secretArgs.some((v) => v.trim().length > 0)
                  ? dto.secretArgs
                  : undefined,
            })
          }}
        />
      ) : null}
    </div>
  )
}

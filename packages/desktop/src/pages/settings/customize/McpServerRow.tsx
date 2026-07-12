import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Plug, Trash2 } from "lucide-react"
import { Badge, Button } from "../../../components/atoms"
import { ConfirmDialog } from "../../../components/molecules"
import { mcpRemove, mcpTest, mcpUpsert, toInvokeError } from "../../../lib/tauri"
import type { McpServerDto } from "../../../lib/types"
import { MCP_SERVERS_KEY } from "./mcpServersKey"

export const McpServerRow = ({ server }: { server: McpServerDto }) => {
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

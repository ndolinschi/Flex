import { useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"

import { ErrorBanner, FieldRow, SettingsSection } from "../../../components/molecules"
import { emptyMcpForm, MCP_ID_RE, parseArgs, parseEnv, splitEnvSecrets, type McpFormState } from "../../../lib/mcp"
import { mcpUpsert, toInvokeError } from "../../../lib/tauri"
import type { McpServerDto } from "../../../lib/types"
import { Input } from "@/components/ui/input"

export const CreateMcpServerForm = ({
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

    const parsed = parseEnv(form.envText)
    const { env, secretEnv } = splitEnvSecrets(parsed)
    const server: McpServerDto = {
      id,
      command: form.command.trim(),
      args: parseArgs(form.args),
      env,
      secretEnv,
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
    <SettingsSection title="New MCP server" className="mb-0">
      <FieldRow
        label="Id"
        htmlFor="mcp-id"
        hint={`Kebab-case, e.g. "filesystem" — tools appear as "${form.id || "id"}__<tool>".`}
      >
        <Input
          id="mcp-id"
          value={form.id}
          onChange={(e) => patch({ id: e.target.value })}
          placeholder="filesystem"
        />
      </FieldRow>

      <FieldRow label="Command" htmlFor="mcp-command">
        <Input
          id="mcp-command"
          value={form.command}
          onChange={(e) => patch({ command: e.target.value })}
          placeholder="npx"
        />
      </FieldRow>

      <FieldRow label="Arguments (optional)" htmlFor="mcp-args" hint="Space-separated.">
        <Input
          id="mcp-args"
          value={form.args}
          onChange={(e) => patch({ args: e.target.value })}
          placeholder="-y @modelcontextprotocol/server-filesystem /path/to/project"
        />
      </FieldRow>

      <FieldRow
        label="Environment variables (optional)"
        htmlFor="mcp-env"
        hint='One "KEY=value" pair per line. Names containing TOKEN, KEY, SECRET, PASSWORD, or AUTH are stored encrypted.'
      >
        <Textarea
          id="mcp-env"
          value={form.envText}
          onChange={(e) => patch({ envText: e.target.value })}
          placeholder={"API_KEY=...\nOTHER_VAR=..."}
          rows={3}
        />
      </FieldRow>

      <FieldRow label="Enabled" htmlFor="mcp-enabled">
        <Switch
          id="mcp-enabled"
          checked={form.enabled}
          onCheckedChange={(checked) => patch({ enabled: checked })}
          aria-label="Enabled"
        />
      </FieldRow>

      {error ? (
        <div className="px-3.5 py-3">
          <ErrorBanner message={error} onDismiss={() => setError(null)} />
        </div>
      ) : null}

      <div className="flex justify-end gap-2 px-3.5 py-3">
        <Button variant="secondary" size="sm" onClick={onCancel}>
          Cancel
        </Button>
        <Button
          size="sm"
          disabled={upsertMutation.isPending}
          onClick={() => void handleSave()}
        >
          {upsertMutation.isPending ? (
            <Spinner data-icon="inline-start" />
          ) : null}
          Save server
        </Button>
      </div>
    </SettingsSection>
  )
}

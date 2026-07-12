import { useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { Button, TextArea, TextInput } from "../../../components/atoms"
import { ErrorBanner, FieldRow, SettingsSection } from "../../../components/molecules"
import { emptyMcpForm, MCP_ID_RE, parseArgs, parseEnv, type McpFormState } from "../../../lib/mcp"
import { mcpUpsert, toInvokeError } from "../../../lib/tauri"
import type { McpServerDto } from "../../../lib/types"

/** Inline create form for a new MCP server — stdio transport only (MVP). */
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

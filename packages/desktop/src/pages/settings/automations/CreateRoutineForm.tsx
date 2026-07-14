import { useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { Button, TextArea, TextInput } from "../../../components/atoms"
import {
  ErrorBanner,
  FieldRow,
  ModelSelect,
  SettingsSection,
} from "../../../components/molecules"
import { useModels } from "../../../hooks/useModels"
import { routinesUpsert, toInvokeError } from "../../../lib/tauri"
import type { RoutineDto } from "../../../lib/types"
import { KEBAB_RE, selectClasses } from "./constants"

type FormState = {
  id: string
  prompt: string
  triggerKind: "cron" | "webhook"
  expr: string
  path: string
  model: string
  cwd: string
  maxIterations: string
  tokenBudget: string
  requireVerification: boolean
}

const emptyForm = (): FormState => ({
  id: "",
  prompt: "",
  triggerKind: "cron",
  expr: "",
  path: "",
  model: "",
  cwd: "",
  maxIterations: "8",
  tokenBudget: "",
  requireVerification: false,
})

/** Inline create form for a new routine — mirrors ProviderSettingsForm's select styling. */
export const CreateRoutineForm = ({
  onCancel,
  onSaved,
}: {
  onCancel: () => void
  onSaved: () => void
}) => {
  const { models, isLoading: modelsLoading } = useModels()
  const [form, setForm] = useState<FormState>(emptyForm())
  const [error, setError] = useState<string | null>(null)

  const upsertMutation = useMutation({
    mutationFn: (routine: RoutineDto) => routinesUpsert(routine),
  })

  const patch = (partial: Partial<FormState>) =>
    setForm((prev) => ({ ...prev, ...partial }))

  const handleSave = async () => {
    setError(null)
    const id = form.id.trim()
    if (!id) {
      setError("Id is required")
      return
    }
    if (!KEBAB_RE.test(id)) {
      setError("Id must be kebab-case (lowercase letters, numbers, hyphens)")
      return
    }
    if (!form.prompt.trim()) {
      setError("Prompt is required")
      return
    }
    if (form.triggerKind === "cron" && !form.expr.trim()) {
      setError("Cron expression is required")
      return
    }
    if (form.triggerKind === "webhook" && !form.path.trim()) {
      setError("Webhook path is required")
      return
    }

    const maxIterations = Number.parseInt(form.maxIterations, 10)
    const tokenBudget = form.tokenBudget.trim()
      ? Number.parseInt(form.tokenBudget, 10)
      : undefined

    const routine: RoutineDto = {
      id,
      prompt: form.prompt.trim(),
      maxIterations: Number.isFinite(maxIterations) && maxIterations > 0 ? maxIterations : 8,
      maxIdenticalFailures: 3,
      tokenBudget,
      requireVerification: form.requireVerification,
      trigger:
        form.triggerKind === "cron"
          ? { kind: "cron", expr: form.expr.trim() }
          : { kind: "webhook", path: form.path.trim() },
      model: form.model.trim() || undefined,
      cwd: form.cwd.trim() || undefined,
    }

    try {
      await upsertMutation.mutateAsync(routine)
      onSaved()
    } catch (err) {
      setError(toInvokeError(err))
    }
  }

  return (
    <SettingsSection title="New automation" className="mb-0">
      <FieldRow label="Id" htmlFor="routine-id" hint='Kebab-case, e.g. "nightly-review"'>
        <TextInput
          id="routine-id"
          value={form.id}
          onChange={(e) => patch({ id: e.target.value })}
          placeholder="nightly-review"
        />
      </FieldRow>

      <FieldRow label="Prompt" htmlFor="routine-prompt">
        <TextArea
          id="routine-prompt"
          value={form.prompt}
          onChange={(e) => patch({ prompt: e.target.value })}
          placeholder="Review overnight PRs opened against main…"
          rows={3}
        />
      </FieldRow>

      <FieldRow label="Trigger" htmlFor="routine-trigger-kind">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <select
            id="routine-trigger-kind"
            value={form.triggerKind}
            onChange={(e) => patch({ triggerKind: e.target.value as "cron" | "webhook" })}
            className={selectClasses}
          >
            <option value="cron">Cron</option>
            <option value="webhook">Webhook</option>
          </select>

          <TextInput
            id="routine-trigger-value"
            value={form.triggerKind === "cron" ? form.expr : form.path}
            onChange={(e) =>
              form.triggerKind === "cron"
                ? patch({ expr: e.target.value })
                : patch({ path: e.target.value })
            }
            placeholder={form.triggerKind === "cron" ? "0 9 * * *" : "/deploy"}
          />
        </div>
      </FieldRow>

      <FieldRow label="Model (optional)" htmlFor="routine-model">
        <ModelSelect
          id="routine-model"
          models={models}
          value={form.model}
          onChange={(value) => patch({ model: value })}
          isLoading={modelsLoading}
          placeholder="Use default model"
        />
      </FieldRow>

      <FieldRow label="Working directory (optional)" htmlFor="routine-cwd">
        <TextInput
          id="routine-cwd"
          value={form.cwd}
          onChange={(e) => patch({ cwd: e.target.value })}
          placeholder="/Users/you/project"
        />
      </FieldRow>

      <FieldRow label="Max iterations" htmlFor="routine-max-iterations">
        <TextInput
          id="routine-max-iterations"
          type="number"
          min={1}
          value={form.maxIterations}
          onChange={(e) => patch({ maxIterations: e.target.value })}
        />
      </FieldRow>

      <FieldRow label="Token budget (optional)" htmlFor="routine-token-budget">
        <TextInput
          id="routine-token-budget"
          type="number"
          min={0}
          value={form.tokenBudget}
          onChange={(e) => patch({ tokenBudget: e.target.value })}
          placeholder="No limit"
        />
      </FieldRow>

      <FieldRow label="Require verification" htmlFor="routine-require-verification">
        <input
          id="routine-require-verification"
          type="checkbox"
          checked={form.requireVerification}
          onChange={(e) => patch({ requireVerification: e.target.checked })}
          className="h-3.5 w-3.5 rounded border-border accent-accent"
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
        <Button size="sm" isLoading={upsertMutation.isPending} onClick={() => void handleSave()}>
          Save automation
        </Button>
      </div>
    </SettingsSection>
  )
}

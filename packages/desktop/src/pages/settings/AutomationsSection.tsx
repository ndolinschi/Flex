import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Clock, Play, Plus, Trash2, Webhook } from "lucide-react"
import { Badge, Button, Spinner, TextArea, TextInput } from "../../components/atoms"
import {
  Collapsible,
  ConfirmDialog,
  ErrorBanner,
  FieldRow,
  ModelSelect,
  SettingsSection,
} from "../../components/molecules"
import { useModels } from "../../hooks/useModels"
import { humanizeCron } from "../../lib/cron"
import {
  routinesHistory,
  routinesList,
  routinesRemove,
  routinesRun,
  routinesUpsert,
  toInvokeError,
} from "../../lib/tauri"
import type { RoutineDto, RoutineRunRecordDto, RoutineTriggerDto } from "../../lib/types"
import { cn, formatRelativeTime } from "../../lib/utils"

const ROUTINES_KEY = ["routines"] as const

const EMPTY_HISTORY: RoutineRunRecordDto[] = []
const EMPTY_ROUTINES: RoutineDto[] = []

const selectClasses = cn(
  "h-8 w-full rounded-md border border-border bg-surface px-2.5 text-sm text-ink",
  "focus:border-accent focus:outline-none focus:[box-shadow:0_0_0_1px_var(--color-accent)]",
)

const KEBAB_RE = /^[a-z0-9]+(-[a-z0-9]+)*$/

/** Trigger summary shown under the routine name: icon + human text. */
const TriggerSummary = ({ trigger }: { trigger: RoutineTriggerDto }) => {
  if (trigger.kind === "cron") {
    return (
      <span className="inline-flex items-center gap-1">
        <Clock className="h-3 w-3" aria-hidden />
        {humanizeCron(trigger.expr ?? "")}
      </span>
    )
  }
  return (
    <span className="inline-flex items-center gap-1">
      <Webhook className="h-3 w-3" aria-hidden />
      POST {trigger.path ?? ""}
    </span>
  )
}

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
const CreateRoutineForm = ({
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
    <SettingsSection title="New automation">
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
        <div className="px-4 py-3">
          <ErrorBanner message={error} onDismiss={() => setError(null)} />
        </div>
      ) : null}

      <div className="flex justify-end gap-2 px-4 py-3">
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

const RoutineRow = ({ routine }: { routine: RoutineDto }) => {
  const queryClient = useQueryClient()
  const [expanded, setExpanded] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [ranNote, setRanNote] = useState(false)

  const historyQuery = useQuery({
    queryKey: ["routine-history", routine.id],
    queryFn: () => routinesHistory(routine.id),
    enabled: expanded,
  })

  const runMutation = useMutation({
    mutationFn: () => routinesRun(routine.id),
    onSuccess: () => {
      setRanNote(true)
      window.setTimeout(() => {
        void queryClient.invalidateQueries({ queryKey: ["sessions"] })
      }, 2_000)
    },
  })

  const removeMutation = useMutation({
    mutationFn: () => routinesRemove(routine.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ROUTINES_KEY })
      setConfirmDelete(false)
    },
  })

  const history = historyQuery.data ?? EMPTY_HISTORY
  const lastRun = history.length > 0 ? history.slice().sort((a, b) => b.startedMs - a.startedMs)[0] : null

  return (
    <div className="flex flex-col">
      <div className="flex items-start gap-3 p-3">
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="min-w-0 flex-1 text-left"
          aria-label={expanded ? "Collapse run history" : "Expand run history"}
          aria-expanded={expanded}
        >
          <p className="truncate text-[13px] text-ink">{routine.title ?? routine.id}</p>
          <p className="mt-0.5 flex items-center gap-1.5 truncate text-[11px] text-ink-muted">
            <TriggerSummary trigger={routine.trigger} />
            <span aria-hidden>·</span>
            <span className="truncate">{routine.prompt}</span>
          </p>
          {ranNote ? (
            <p className="mt-1 text-xs text-accent">
              Started — a new session will appear in the sidebar.
            </p>
          ) : null}
        </button>

        <div className="flex shrink-0 items-center gap-1.5">
          {historyQuery.isSuccess && lastRun ? (
            <Badge variant={lastRun.stopReason === "completed" ? "success" : "muted"}>
              {lastRun.stopReason}
            </Badge>
          ) : null}
          <Button
            variant="ghost"
            size="sm"
            isLoading={runMutation.isPending}
            onClick={() => void runMutation.mutateAsync()}
          >
            <Play className="h-3 w-3" aria-hidden /> Run now
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

      <Collapsible open={expanded}>
        <div className="border-t border-stroke-3 px-3 py-2">
          {historyQuery.isLoading ? (
            <div className="flex items-center gap-2 py-2 text-sm text-ink-muted">
              <Spinner size="sm" /> Loading history…
            </div>
          ) : history.length === 0 ? (
            <p className="py-2 text-sm text-ink-faint">No runs yet.</p>
          ) : (
            <div className="flex flex-col gap-1.5 py-1">
              {history
                .slice()
                .sort((a, b) => b.startedMs - a.startedMs)
                .map((record) => (
                  <div
                    key={`${record.sessionId}-${record.startedMs}`}
                    className="flex items-center gap-2 text-xs"
                  >
                    <span className="text-ink-muted">
                      {formatRelativeTime(record.startedMs)}
                    </span>
                    <Badge
                      variant={record.stopReason === "completed" ? "success" : "muted"}
                    >
                      {record.stopReason}
                    </Badge>
                    <span className="text-ink-faint">
                      {record.iterations} iteration{record.iterations === 1 ? "" : "s"}
                    </span>
                  </div>
                ))}
            </div>
          )}
        </div>
      </Collapsible>

      <ConfirmDialog
        open={confirmDelete}
        title={`Delete "${routine.title ?? routine.id}"?`}
        description="This removes the automation. Existing run history is deleted too."
        confirmLabel="Delete"
        danger
        isLoading={removeMutation.isPending}
        onConfirm={() => void removeMutation.mutateAsync()}
        onCancel={() => setConfirmDelete(false)}
      />
    </div>
  )
}

/** Automations content — scheduled/webhook-triggered routines (cron/webhook
 * run_goal). Mounted inside the Settings shell's "Automations" section
 * (design-map/07-settings.md build brief §3); no `SettingsShell` wrapper
 * here anymore since the shell owns nav+header+page title. */
export const AutomationsContent = () => {
  const [creating, setCreating] = useState(false)
  const queryClient = useQueryClient()

  const routinesQuery = useQuery({
    queryKey: ROUTINES_KEY,
    queryFn: routinesList,
  })

  const routines = routinesQuery.data ?? EMPTY_ROUTINES

  const newAutomationButton = (
    <Button size="sm" onClick={() => setCreating(true)}>
      <Plus className="h-3.5 w-3.5" aria-hidden /> New automation
    </Button>
  )

  return (
    <div className="flex flex-col gap-4">
      <SettingsSection
        title="Routines"
        description="Run on a schedule or webhook and start a new session automatically"
        actions={!creating ? newAutomationButton : undefined}
        className="mb-0"
        rowId="automations-routines"
      >
        {routinesQuery.isLoading ? (
          <div className="flex items-center gap-2 p-3 text-sm text-ink-muted">
            <Spinner size="sm" /> Loading automations…
          </div>
        ) : routinesQuery.isError ? (
          <div className="p-3">
            <ErrorBanner message={toInvokeError(routinesQuery.error)} />
          </div>
        ) : routines.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-2 py-12 text-center">
            <p className="text-[13px] text-ink-secondary">No automations yet</p>
            <p className="text-xs text-ink-faint">
              Create an automation to run a prompt on a schedule or webhook.
            </p>
            {newAutomationButton}
          </div>
        ) : (
          routines.map((routine) => <RoutineRow key={routine.id} routine={routine} />)
        )}
      </SettingsSection>

      {creating ? (
        <CreateRoutineForm
          onCancel={() => setCreating(false)}
          onSaved={() => {
            setCreating(false)
            void queryClient.invalidateQueries({ queryKey: ROUTINES_KEY })
          }}
        />
      ) : null}
    </div>
  )
}

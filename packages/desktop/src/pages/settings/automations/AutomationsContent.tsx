import { useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { Plus } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Spinner } from "../../../components/atoms"
import { ErrorBanner, EmptyState, SettingsSection } from "../../../components/molecules"
import { routinesList, toInvokeError } from "../../../lib/tauri"
import { CreateRoutineForm } from "./CreateRoutineForm"
import { EMPTY_ROUTINES, ROUTINES_KEY } from "./constants"
import { RoutineRow } from "./RoutineRow"

/** Automations content — scheduled/webhook-triggered routines (cron/webhook
 * run_goal). Mounted inside the Settings shell's "Automations" section
 * (DESIGN.md Settings); no `SettingsShell` wrapper
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
    <div className="flex flex-col gap-3">
      <SettingsSection
        title="Routines"
        description="Run on a schedule or webhook and start a new session automatically"
        actions={!creating ? newAutomationButton : undefined}
        className="mb-0"
        rowId="automations-routines"
      >
        {routinesQuery.isLoading ? (
          <div className="flex items-center gap-2 px-3.5 py-3 text-sm text-ink-muted">
            <Spinner size="sm" /> Loading automations…
          </div>
        ) : routinesQuery.isError ? (
          <div className="px-3.5 py-3">
            <ErrorBanner message={toInvokeError(routinesQuery.error)} />
          </div>
        ) : routines.length === 0 ? (
          <EmptyState
            title="No automations yet"
            description="Create an automation to run a prompt on a schedule or webhook."
            actionLabel="New automation"
            onAction={() => setCreating(true)}
          />
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

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Play, Trash2 } from "lucide-react"
import { Badge, Spinner } from "../../../components/atoms"
import { Button } from "@/components/ui/button"
import { Button as ShadcnButton } from "@/components/ui/button"
import {
  Collapsible,
  ConfirmDialog,
} from "../../../components/molecules"
import { routinesHistory, routinesRemove, routinesRun } from "../../../lib/tauri"
import type { RoutineDto } from "../../../lib/types"
import { formatRelativeTime } from "../../../lib/utils"
import { EMPTY_HISTORY, ROUTINES_KEY } from "./constants"
import { TriggerSummary } from "./TriggerSummary"

export const RoutineRow = ({ routine }: { routine: RoutineDto }) => {
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
      <div className="flex items-start gap-3 px-3.5 py-3">
        <ShadcnButton
          variant="ghost"
          onClick={() => setExpanded((v) => !v)}
          className="h-auto min-w-0 flex-1 justify-start px-0 py-0 font-normal hover:bg-transparent"
          aria-label={expanded ? "Collapse run history" : "Expand run history"}
          aria-expanded={expanded}
        >
          <p className="truncate text-base text-ink">{routine.title ?? routine.id}</p>
          <p className="mt-0.5 flex items-center gap-1.5 truncate text-xs text-ink-muted">
            <TriggerSummary trigger={routine.trigger} />
            <span aria-hidden>·</span>
            <span className="truncate">{routine.prompt}</span>
          </p>
          {ranNote ? (
            <p className="mt-1 text-xs text-accent">
              Started — a new session will appear in the sidebar.
            </p>
          ) : null}
        </ShadcnButton>

        <div className="flex shrink-0 items-center gap-1.5">
          {historyQuery.isSuccess && lastRun ? (
            <Badge variant={lastRun.stopReason === "completed" ? "success" : "muted"}>
              {lastRun.stopReason}
            </Badge>
          ) : null}
          <Button
            variant="ghost"
            size="sm"
            disabled={runMutation.isPending}
            onClick={() => void runMutation.mutateAsync()}
          >
            {runMutation.isPending ? <Spinner data-icon="inline-start" /> : null}
            <Play className="h-3 w-3" aria-hidden /> Run now
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

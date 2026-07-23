import { useQuery } from "@tanstack/react-query"
import { GitFork } from "lucide-react"
import type { IsolationPolicy } from "../../../lib/types"
import { listSessions, listWorkspaces, type WorkspaceInfo } from "../../../lib/tauri"
import { useAppStore } from "../../../stores/appStore"
import { SESSIONS_KEY } from "../../../hooks/useSessions"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { contextBarTriggerClass } from "./chrome"

const ISOLATION_OPTIONS: {
  value: IsolationPolicy
  label: string
  description: string
}[] = [
  {
    value: "never",
    label: "Direct",
    description: "Runs in your project folder; edits files in place.",
  },
  {
    value: "required",
    label: "Isolated",
    description:
      "Runs in a separate git worktree; workspace is created on the first prompt (or reuse an existing one below).",
  },
]

const NEW_WORKSPACE_VALUE = "__new__"

const policyLabel = (policy: IsolationPolicy | null | undefined): string => {
  const value = policy === "required" ? "required" : "never"
  return ISOLATION_OPTIONS.find((o) => o.value === value)!.label
}

export const IsolationPicker = ({
  sessionId,
  projectCwd,
  disabled,
}: {
  sessionId: string
  projectCwd?: string
  disabled?: boolean
}) => {
  const selectedIsolation = useAppStore((s) => s.selectedIsolation)
  const setSelectedIsolation = useAppStore((s) => s.setSelectedIsolation)
  const selectedReuseWorkspaceId = useAppStore(
    (s) => s.selectedReuseWorkspaceId,
  )
  const setSelectedReuseWorkspaceId = useAppStore(
    (s) => s.setSelectedReuseWorkspaceId,
  )
  const hasTurnUsage = !!useAppStore((s) => s.lastTurnUsage[sessionId])
  const logRowCount = useAppStore(
    (s) => s.sessionLogRows[sessionId]?.length ?? 0,
  )
  const hasTurns = hasTurnUsage || logRowCount > 0

  const { data: sessions } = useQuery({
    queryKey: SESSIONS_KEY,
    queryFn: listSessions,
    staleTime: 30_000,
  })
  const sessionPolicy = sessions?.find((s) => s.id === sessionId)?.isolation

  const draftPolicy: IsolationPolicy =
    selectedIsolation === "required" ? "required" : "never"
  const current: IsolationPolicy = hasTurns
    ? sessionPolicy === "required"
      ? "required"
      : "never"
    : draftPolicy
  const currentLabel = policyLabel(current)

  const shouldListWorkspaces =
    !hasTurns && current === "required" && !!projectCwd
  const { data: workspaces = [] } = useQuery<WorkspaceInfo[]>({
    queryKey: ["list-workspaces", projectCwd ?? ""],
    queryFn: () => listWorkspaces(projectCwd!),
    enabled: shouldListWorkspaces,
    staleTime: 10_000,
  })

  if (hasTurns) {
    return (
      <span
        className="ml-1 flex h-5 items-center gap-1 rounded-md px-1.5 text-sm text-ink-muted opacity-60"
        title="Isolation is fixed for this session"
      >
        <GitFork className="size-3 shrink-0" aria-hidden />
        {currentLabel}
      </span>
    )
  }

  const reuseValue = selectedReuseWorkspaceId ?? NEW_WORKSPACE_VALUE

  return (
    <>
      <Select
        items={ISOLATION_OPTIONS}
        value={current}
        disabled={disabled}
        onValueChange={(v) => {
          if (v == null) return
          setSelectedIsolation(v as IsolationPolicy)
          if (v === "never") setSelectedReuseWorkspaceId(null)
        }}
      >
        <SelectTrigger
          aria-label={`Isolation: ${currentLabel}`}
          className={contextBarTriggerClass("ml-0.5 data-[size=xs]:rounded-md data-[size=xs]:pr-1.5")}
          size="xs"
        >
          <GitFork className="size-3 shrink-0" aria-hidden />
          <SelectValue />
        </SelectTrigger>
        <SelectContent align="start" className="w-72">
          <SelectGroup>
            <SelectLabel>Session isolation</SelectLabel>
            {ISOLATION_OPTIONS.map((opt) => (
              <SelectItem key={opt.value} value={opt.value}>
                <span className="flex flex-col gap-0.5">
                  <span>{opt.label}</span>
                  <span className="text-xs text-ink-muted">
                    {opt.description}
                  </span>
                </span>
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      {current === "required" && projectCwd ? (
        <Select
          items={[
            { value: NEW_WORKSPACE_VALUE, label: "New workspace" },
            ...workspaces.map((w) => ({ value: w.id, label: w.id })),
          ]}
          value={reuseValue}
          disabled={disabled}
          onValueChange={(v) => {
            if (v == null) return
            setSelectedReuseWorkspaceId(v === NEW_WORKSPACE_VALUE ? null : v)
          }}
        >
          <SelectTrigger
            aria-label="Reuse workspace"
            className={contextBarTriggerClass(
              "max-w-[9rem] data-[size=xs]:rounded-md data-[size=xs]:pr-1.5",
            )}
            size="xs"
          >
            <SelectValue placeholder="New workspace" />
          </SelectTrigger>
          <SelectContent align="start" className="w-72">
            <SelectGroup>
              <SelectLabel>Workspace</SelectLabel>
              <SelectItem value={NEW_WORKSPACE_VALUE}>
                <span className="flex flex-col gap-0.5">
                  <span>New workspace</span>
                  <span className="text-xs text-ink-muted">
                    Provision a fresh worktree on the first prompt.
                  </span>
                </span>
              </SelectItem>
              {workspaces.map((w) => (
                <SelectItem key={w.id} value={w.id}>
                  <span className="flex flex-col gap-0.5">
                    <span>{w.id}</span>
                    <span className="truncate text-xs text-ink-muted">
                      {w.path}
                    </span>
                  </span>
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
      ) : null}
    </>
  )
}

import { GitFork } from "lucide-react"
import type { IsolationPolicy } from "../../../lib/types"
import { useAppStore } from "../../../stores/appStore"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

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
    description: "Runs in a separate git worktree; changes reviewed before merging.",
  },
]

/**
 * Draft-only picker: chooses the isolation for the NEXT session this draft
 * turns into. Isolation is fixed at `create_session` time — there's no
 * `update_session` patch for it (see `commands.rs::UpdateSessionInput`) — so
 * this can't reconfigure the current draft in place. Instead the picker
 * writes a store preference (`selectedIsolation`) that `newAgentCreateInput`
 * / `ProjectPicker`'s create-session path read when they next call
 * `create_session`. Once the draft has taken its first turn the choice is
 * locked in — the picker turns into a plain read-only indicator here, and
 * once the session IS isolated `IsolationBadge` above takes over as the one
 * true indicator (this component doesn't render for isolated sessions).
 */
export const IsolationPicker = ({
  sessionId,
  disabled,
}: {
  sessionId: string
  disabled?: boolean
}) => {
  const selectedIsolation = useAppStore((s) => s.selectedIsolation)
  const setSelectedIsolation = useAppStore((s) => s.setSelectedIsolation)
  // Both selectors must run unconditionally on every render — `||` short-
  // circuits, so inlining a second `useAppStore` call on its right-hand side
  // would skip that hook call whenever the first is truthy, changing the
  // number of hooks called between renders ("Rendered fewer hooks than
  // expected"). Read both values first, then combine.
  const hasTurnUsage = !!useAppStore((s) => s.lastTurnUsage[sessionId])
  const logRowCount = useAppStore(
    (s) => s.sessionLogRows[sessionId]?.length ?? 0,
  )
  const hasTurns = hasTurnUsage || logRowCount > 0

  const current = selectedIsolation === "required" ? "required" : "never"
  const currentLabel = ISOLATION_OPTIONS.find((o) => o.value === current)!.label

  // Once the draft has had a turn, the choice that produced this session is
  // final — show a static label instead of an editable picker so it's clear
  // selecting no longer does anything.
  if (hasTurns) {
    return (
      <span
        className="ml-1 flex h-6 items-center gap-1 rounded-md px-1.5 text-sm text-muted-foreground opacity-60"
        title="Isolation is fixed for this session"
      >
        <GitFork className="size-3 shrink-0" aria-hidden />
        {currentLabel}
      </span>
    )
  }

  return (
    <Select
      items={ISOLATION_OPTIONS}
      value={current}
      disabled={disabled}
      onValueChange={(v) => {
        if (v == null) return
        setSelectedIsolation(v as IsolationPolicy)
      }}
    >
      <SelectTrigger
        aria-label={`Isolation: ${currentLabel}`}
        className="ml-1 h-6 w-auto gap-1 rounded-md border-0 bg-transparent px-1.5 text-sm font-normal text-muted-foreground opacity-80 shadow-none hover:bg-transparent hover:text-foreground hover:opacity-100 data-[size=sm]:h-6 data-open:opacity-100"
        size="sm"
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
                <span className="text-xs text-muted-foreground">
                  {opt.description}
                </span>
              </span>
            </SelectItem>
          ))}
        </SelectGroup>
      </SelectContent>
    </Select>
  )
}

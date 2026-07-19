import { useState } from "react"
import { Check, GitFork } from "lucide-react"
import type { IsolationPolicy } from "../../../lib/types"
import { useAppStore } from "../../../stores/appStore"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

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
  const [open, setOpen] = useState(false)
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
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger
        disabled={disabled}
        render={
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={disabled}
            aria-label={`Isolation: ${currentLabel}`}
            className="ml-1 h-6 gap-1 rounded-md px-1.5 text-sm font-normal text-muted-foreground opacity-80 hover:bg-transparent hover:text-foreground hover:opacity-100 aria-expanded:opacity-100"
          />
        }
      >
        <GitFork className="size-3 shrink-0" aria-hidden />
        <span className="min-w-0 truncate">{currentLabel}</span>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" sideOffset={6} className="w-72">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Session isolation</DropdownMenuLabel>
          {ISOLATION_OPTIONS.map((opt) => {
            const active = opt.value === current
            return (
              <DropdownMenuItem
                key={opt.value}
                className="items-start gap-2"
                onClick={() => {
                  setSelectedIsolation(opt.value)
                  setOpen(false)
                }}
              >
                <span className="min-w-0 flex-1">
                  <span className="flex items-center gap-1.5 text-sm text-foreground">
                    {opt.label}
                    {active ? (
                      <Check className="size-3 text-primary" aria-hidden />
                    ) : null}
                  </span>
                  <span className="mt-0.5 block text-xs text-muted-foreground">
                    {opt.description}
                  </span>
                </span>
              </DropdownMenuItem>
            )
          })}
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

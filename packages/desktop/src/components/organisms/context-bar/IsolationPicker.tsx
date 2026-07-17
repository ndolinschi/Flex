import { useState } from "react"
import { Check, GitFork } from "@/components/icons"
import type { IsolationPolicy } from "../../../lib/types"
import { useAppStore } from "../../../stores/appStore"
import { cn } from "../../../lib/utils"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { PopoverItem } from "../../molecules/PopoverTray"

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
        className="ml-1 flex h-6 items-center gap-1 rounded-md px-1.5 text-sm text-ink-muted opacity-60"
        title="Isolation is fixed for this session"
      >
        <GitFork className="h-3 w-3 shrink-0" aria-hidden />
        {currentLabel}
      </span>
    )
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          disabled={disabled}
          aria-haspopup="listbox"
          aria-expanded={open}
          aria-label={`Isolation: ${currentLabel}`}
          className={cn(
            "ml-1 flex h-6 items-center gap-1 rounded-md px-1.5",
            "text-sm text-ink-muted opacity-80",
            "transition-[color,opacity] duration-[var(--duration-fast)]",
            "hover:text-ink-secondary hover:opacity-100 disabled:opacity-50",
            open && "opacity-100",
          )}
        >
          <GitFork className="h-3 w-3 shrink-0" aria-hidden />
          <span className="min-w-0 truncate">{currentLabel}</span>
        </button>
      </PopoverTrigger>

      <PopoverContent
        side="top"
        align="start"
        sideOffset={6}
        role="listbox"
        aria-label="Session isolation"
        className="w-72 gap-0 rounded-md border-0 bg-panel p-0 shadow-[var(--shadow-popover)] ring-0"
      >
        <ul className="py-0.5">
          {ISOLATION_OPTIONS.map((opt) => {
            const active = opt.value === current
            return (
              <li key={opt.value}>
                <PopoverItem
                  active={active}
                  onClick={() => {
                    setSelectedIsolation(opt.value)
                    setOpen(false)
                  }}
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5">
                      <span className="truncate">{opt.label}</span>
                      {active ? (
                        <Check className="h-3 w-3 shrink-0 text-accent" aria-hidden />
                      ) : null}
                    </div>
                    <p className="mt-0.5 truncate text-xs text-ink-muted">
                      {opt.description}
                    </p>
                  </div>
                </PopoverItem>
              </li>
            )
          })}
        </ul>
      </PopoverContent>
    </Popover>
  )
}

import { useEffect, useMemo, useRef, useState } from "react"
import {
  Bug,
  Check,
  ChevronDown,
  ListTodo,
  MessageCircle,
  Network,
  Sparkles,
} from "lucide-react"
import type { ComposerMode, PermissionMode } from "../../lib/types"
import { FLEX_MODE_ENABLED } from "../../lib/featureFlags"
import { cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import { Button } from "@/components/ui/button"
import { PopoverItem, PopoverTray } from "./PopoverTray"

type ModeOption = {
  id: ComposerMode
  label: string
  description: string
  icon: typeof Sparkles
  accent: string
}

const MODES: ModeOption[] = [
  {
    id: "agent",
    label: "Agent",
    description: "Build and edit with full tools",
    icon: Sparkles,
    accent: "text-green",
  },
  {
    id: "plan",
    label: "Plan",
    description: "Design before coding (read-only)",
    icon: ListTodo,
    accent: "text-yellow",
  },
  {
    id: "ask",
    label: "Ask",
    description: "Questions without making changes",
    icon: MessageCircle,
    accent: "text-cyan",
  },
  {
    id: "debug",
    label: "Debug",
    description:
      "Reproduce → probe → fix → clean: temporary debug code, then remove it",
    icon: Bug,
    accent: "text-orange",
  },
  {
    id: "flex",
    label: "Flex",
    description: "Orchestrates planning, review, and isolated workers across models",
    icon: Network,
    accent: "text-purple",
  },
]

/** Modes shown in the picker — Flex gated by `FLEX_MODE_ENABLED`. */
export const visibleComposerModes = (): ModeOption[] =>
  MODES.filter((mode) => mode.id !== "flex" || FLEX_MODE_ENABLED)

type ModePickerProps = {
  value: ComposerMode
  onChange: (mode: ComposerMode) => void
  disabled?: boolean
}

/** Agent / Plan / Ask (/ Flex when flagged) mode pill for the composer footer. */
export const ModePicker = ({ value, onChange, disabled }: ModePickerProps) => {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  const modes = useMemo(() => visibleComposerModes(), [])
  const effectiveValue =
    value === "flex" && !FLEX_MODE_ENABLED ? "agent" : value
  const selected = modes.find((m) => m.id === effectiveValue) ?? modes[0]
  const Icon = selected.icon

  // Persisted Flex mode while the flag is off → fall back to Agent.
  useEffect(() => {
    if (value === "flex" && !FLEX_MODE_ENABLED) onChange("agent")
  }, [value, onChange])

  return (
    <div ref={rootRef} className="relative">
      <Button
        variant="ghost"
        size="xs"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`Mode: ${selected.label}`}
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "rounded-full border border-stroke-3 bg-fill-4 px-2",
          "tracking-[var(--tracking-caption)] text-ink-secondary",
          "transition-[background-color,border-color] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
          "hover:border-stroke-2 hover:bg-fill-2 hover:text-ink-secondary",
          open && "border-stroke-2 bg-fill-2",
        )}
      >
        <Icon className={cn("h-3 w-3", selected.accent)} aria-hidden />
        <span className="font-medium">{selected.label}</span>
        <ChevronDown className="h-2.5 w-2.5 opacity-60" aria-hidden />
      </Button>

      <PopoverTray
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={rootRef}
        placement="above"
        role="listbox"
        aria-label="Composer mode"
        className="left-0 w-56"
      >
        {modes.map((mode) => {
          const ModeIcon = mode.icon
          const isActive = mode.id === effectiveValue
          return (
            <PopoverItem
              key={mode.id}
              onClick={() => {
                onChange(mode.id)
                // Plan mode should surface the Plan tab even when empty —
                // previously the tab only appeared after ExitPlanMode.
                if (mode.id === "plan") {
                  useAppStore.getState().revealPlanPanel()
                }
                setOpen(false)
              }}
              className="items-start py-2"
            >
              <ModeIcon
                className={cn("mt-0.5 h-3.5 w-3.5 shrink-0", mode.accent)}
                aria-hidden
              />
              <span className="min-w-0 flex-1">
                <span className="flex items-center gap-1.5 text-base text-ink">
                  {mode.label}
                  {isActive ? (
                    <Check className="h-3 w-3 text-accent" aria-hidden />
                  ) : null}
                </span>
                <span className="block text-sm text-ink-muted">
                  {mode.description}
                </span>
              </span>
            </PopoverItem>
          )
        })}
      </PopoverTray>
    </div>
  )
}

export const modePlaceholder = (mode: ComposerMode, isHero: boolean): string => {
  if (!isHero) {
    if (mode === "plan") return "Refine the plan…"
    if (mode === "ask") return "Ask a follow-up…"
    if (mode === "debug") return "Describe the failure or next probe…"
    if (mode === "flex") return "Direct the orchestrator…"
    return "Send follow-up"
  }
  if (mode === "plan") return "Plan and design before coding…"
  if (mode === "ask") return "Ask questions without making changes…"
  if (mode === "debug")
    return "Describe the bug — Debug reproduces, probes, fixes, then cleans up…"
  if (mode === "flex") return "Describe the task — Flex plans, reviews, and executes it…"
  return "Plan, search, build anything"
}

/** Agent/Debug defer to the user's configured default (Settings → Behavior →
 * Permissions, `appStore.defaultPermissionMode`) — read live via
 * `getState()` since this is a plain function, not a component/hook, and
 * callers (Composer.tsx, usePlanBuild.ts) invoke it at turn-submit time, not
 * render time. Plan/Ask/Flex keep their own fixed safeguards regardless of
 * that setting. */
export const modeToPermission = (mode: ComposerMode): PermissionMode => {
  if (mode === "plan") return "plan"
  if (mode === "ask") return "dont_ask"
  if (mode === "flex") return "dont_ask"
  return useAppStore.getState().defaultPermissionMode
}

/** Bypass-permissions shield applies in Agent and Debug (full-tool modes). */
export const modeAllowsBypass = (mode: ComposerMode | string): boolean =>
  mode === "agent" || mode === "debug"

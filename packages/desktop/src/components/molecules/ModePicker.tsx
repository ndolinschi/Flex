import { useEffect, useMemo } from "react"
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

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

/** Agent / Plan / Ask (/ Flex when flagged) mode picker for the composer footer. */
export const ModePicker = ({ value, onChange, disabled }: ModePickerProps) => {
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
    <DropdownMenu>
      <DropdownMenuTrigger
        disabled={disabled}
        render={
          <Button
            variant="outline"
            size="sm"
            disabled={disabled}
            aria-label={`Mode: ${selected.label}`}
          />
        }
      >
        <Icon className={cn("size-3.5", selected.accent)} aria-hidden />
        {selected.label}
        <ChevronDown data-icon="inline-end" className="opacity-60" aria-hidden />
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="start" className="w-56">
        <DropdownMenuGroup>
          {modes.map((mode) => {
            const ModeIcon = mode.icon
            const isActive = mode.id === effectiveValue
            return (
              <DropdownMenuItem
                key={mode.id}
                className="items-start py-2"
                onClick={() => {
                  onChange(mode.id)
                  if (mode.id === "plan") {
                    useAppStore.getState().revealPlanPanel()
                  }
                }}
              >
                <ModeIcon
                  className={cn("mt-0.5 size-3.5 shrink-0", mode.accent)}
                  aria-hidden
                />
                <span className="min-w-0 flex-1 text-left">
                  <span className="flex items-center gap-1.5 text-sm text-foreground">
                    {mode.label}
                    {isActive ? (
                      <Check className="size-3 text-primary" aria-hidden />
                    ) : null}
                  </span>
                  <span className="block text-xs text-muted-foreground">
                    {mode.description}
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

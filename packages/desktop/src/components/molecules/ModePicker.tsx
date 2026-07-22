import { useEffect, useMemo } from "react"
import {
  Bug,
  ListTodo,
  MessageCircle,
  Network,
  Sparkles,
} from "lucide-react"
import type { ComposerMode, PermissionMode } from "../../lib/types"
import { FLEX_MODE_ENABLED } from "../../lib/featureFlags"
import { cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

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
    description: "Reproduce, probe, fix, then clean up",
    icon: Bug,
    accent: "text-orange",
  },
  {
    id: "flex",
    label: "Flex",
    description: "Plan, review, and run isolated workers",
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
  const items = useMemo(
    () => modes.map((m) => ({ label: m.label, value: m.id })),
    [modes],
  )
  const effectiveValue =
    value === "flex" && !FLEX_MODE_ENABLED ? "agent" : value
  const selected = modes.find((m) => m.id === effectiveValue) ?? modes[0]
  const Icon = selected.icon

  // Persisted Flex mode while the flag is off → fall back to Agent.
  useEffect(() => {
    if (value === "flex" && !FLEX_MODE_ENABLED) onChange("agent")
  }, [value, onChange])

  return (
    <Select
      items={items}
      value={effectiveValue}
      disabled={disabled}
      onValueChange={(next) => {
        if (next == null) return
        const mode = next as ComposerMode
        onChange(mode)
        if (mode === "plan") {
          useAppStore.getState().revealPlanPanel()
        }
      }}
    >
      <SelectTrigger
        size="xs"
        aria-label={`Mode: ${selected.label}`}
        className="border-0 bg-transparent shadow-none opacity-70 hover:bg-transparent hover:opacity-100 data-open:bg-transparent data-open:opacity-100"
      >
        <Icon className={cn("size-3.5", selected.accent)} aria-hidden />
        <SelectValue />
      </SelectTrigger>
      <SelectContent side="top" align="start" alignItemWithTrigger={false} className="min-w-64">
        <SelectGroup>
          {modes.map((mode) => {
            const ModeIcon = mode.icon
            return (
              <SelectItem key={mode.id} value={mode.id} className="items-start py-2">
                <ModeIcon
                  className={cn("mt-0.5 size-3.5 shrink-0", mode.accent)}
                  aria-hidden
                />
                <span className="min-w-0 flex-1 text-left">
                  <span className="block text-sm text-foreground">{mode.label}</span>
                  <span className="block whitespace-normal text-xs text-muted-foreground">
                    {mode.description}
                  </span>
                </span>
              </SelectItem>
            )
          })}
        </SelectGroup>
      </SelectContent>
    </Select>
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

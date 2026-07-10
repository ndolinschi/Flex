import { useRef, useState } from "react"
import {
  Check,
  ChevronDown,
  ListTodo,
  MessageCircle,
  Sparkles,
} from "lucide-react"
import type { ComposerMode } from "../../lib/types"
import { cn } from "../../lib/utils"
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
]

type ModePickerProps = {
  value: ComposerMode
  onChange: (mode: ComposerMode) => void
  disabled?: boolean
}

/** Cursor-style Agent / Plan / Ask mode pill for the composer footer. */
export const ModePicker = ({ value, onChange, disabled }: ModePickerProps) => {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  const selected = MODES.find((m) => m.id === value) ?? MODES[0]
  const Icon = selected.icon

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`Mode: ${selected.label}`}
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "flex items-center gap-1 rounded-full border border-stroke-3 bg-fill-3 px-2 py-[2px]",
          "text-sm text-ink-secondary",
          "transition-[background-color,border-color] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
          "hover:border-stroke-2 hover:bg-fill-2 disabled:opacity-50",
          open && "border-stroke-2 bg-fill-2",
        )}
      >
        <Icon className={cn("h-3 w-3", selected.accent)} aria-hidden />
        <span>{selected.label}</span>
        <ChevronDown className="h-2.5 w-2.5 opacity-60" aria-hidden />
      </button>

      <PopoverTray
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={rootRef}
        placement="above"
        role="listbox"
        aria-label="Composer mode"
        className="left-0 w-56"
      >
        {MODES.map((mode) => {
          const ModeIcon = mode.icon
          const isActive = mode.id === value
          return (
            <PopoverItem
              key={mode.id}
              active={isActive}
              onClick={() => {
                onChange(mode.id)
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
    return "Send follow-up"
  }
  if (mode === "plan") return "Plan and design before coding…"
  if (mode === "ask") return "Ask questions without making changes…"
  return "Plan, search, build anything"
}

export const modeToPermission = (
  mode: ComposerMode,
): "default" | "plan" | "dont_ask" => {
  if (mode === "plan") return "plan"
  if (mode === "ask") return "dont_ask"
  return "default"
}

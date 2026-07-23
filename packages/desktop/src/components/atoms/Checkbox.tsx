import type { MouseEvent } from "react"
import { Checkbox as CheckboxPrimitive } from "@base-ui/react/checkbox"
import { Check, Minus } from "lucide-react"
import { cn } from "@/lib/utils"

type CheckboxProps = {
  checked: boolean
  indeterminate?: boolean
  onChange: (checked: boolean) => void
  label: string
  disabled?: boolean
  className?: string
  onClick?: (e: MouseEvent) => void
}

export const Checkbox = ({
  checked,
  indeterminate = false,
  onChange,
  label,
  disabled,
  className,
  onClick,
}: CheckboxProps) => (
  <CheckboxPrimitive.Root
    checked={checked}
    indeterminate={indeterminate}
    onCheckedChange={(value) => onChange(value === true)}
    aria-label={label}
    title={label}
    disabled={disabled}
    onClick={onClick}
    className={cn(
      "peer relative flex size-4 shrink-0 items-center justify-center rounded-full border border-stroke-3 bg-elevated outline-none",
      "transition-[color,background-color,border-color] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
      "after:absolute after:-inset-x-3 after:-inset-y-2",
      "focus-visible:border-stroke-2 focus-visible:ring-1 focus-visible:ring-stroke-2",
      "disabled:cursor-not-allowed disabled:opacity-50",
      "aria-invalid:border-destructive aria-invalid:ring-1 aria-invalid:ring-destructive/30",
      "data-checked:border-primary data-checked:bg-primary data-checked:text-primary-foreground",
      "data-indeterminate:border-primary data-indeterminate:bg-primary data-indeterminate:text-primary-foreground",
      className,
    )}
  >
    <CheckboxPrimitive.Indicator className="grid place-content-center text-current transition-none [&>svg]:size-3">
      {indeterminate && !checked ? (
        <Minus strokeWidth={3} aria-hidden />
      ) : (
        <Check strokeWidth={3} aria-hidden />
      )}
    </CheckboxPrimitive.Indicator>
  </CheckboxPrimitive.Root>
)

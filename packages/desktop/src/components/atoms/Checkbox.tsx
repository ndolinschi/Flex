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

/** Selection control — uses the Base UI Checkbox primitive (same as
 * `@/components/ui/checkbox`) with a custom indicator for indeterminate state.
 * Round shape preserved; shadcn accent tokens applied. */
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
      "peer relative flex size-4 shrink-0 items-center justify-center rounded-full border border-input transition-colors outline-none",
      "focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50",
      "disabled:cursor-not-allowed disabled:opacity-50",
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

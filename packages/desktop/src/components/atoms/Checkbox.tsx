import type { ButtonHTMLAttributes } from "react"
import { Check, Minus } from "lucide-react"
import { cn } from "../../lib/utils"

type CheckboxProps = Omit<
  ButtonHTMLAttributes<HTMLButtonElement>,
  "onChange" | "role" | "aria-checked"
> & {
  checked: boolean
  /** Partial selection (select-all when some but not all rows are checked). */
  indeterminate?: boolean
  onChange: (checked: boolean) => void
  label: string
}

/** Round selection control — filled accent circle + check when on, hairline
 * ring when off. Used by Changes select-all / file rows (not a settings switch;
 * use `Toggle` for binary prefs). */
export const Checkbox = ({
  checked,
  indeterminate = false,
  onChange,
  label,
  disabled,
  className,
  onClick,
  ...props
}: CheckboxProps) => {
  const on = checked || indeterminate
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={indeterminate ? "mixed" : checked}
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={(e) => {
        onClick?.(e)
        if (!e.defaultPrevented) onChange(!checked)
      }}
      className={cn(
        "inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full",
        "border transition-[color,background-color,border-color,box-shadow]",
        "duration-[var(--duration-fast)] ease-[var(--easing-default)]",
        "disabled:cursor-not-allowed disabled:opacity-50",
        on
          ? "border-accent bg-accent text-accent-text shadow-[0_0_0_1px_var(--color-accent)]"
          : "border-stroke-2 bg-transparent text-transparent hover:border-stroke-1 hover:bg-fill-3",
        className,
      )}
      {...props}
    >
      {indeterminate && !checked ? (
        <Minus className="h-2.5 w-2.5" strokeWidth={3} aria-hidden />
      ) : (
        <Check
          className={cn("h-2.5 w-2.5", !checked && "opacity-0")}
          strokeWidth={3}
          aria-hidden
        />
      )}
    </button>
  )
}

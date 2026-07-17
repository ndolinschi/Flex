import type { ButtonHTMLAttributes } from "react"
import { Checkbox as UiCheckbox } from "@/components/ui/checkbox"
import { cn } from "@/lib/utils"

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

/** Round Changes-panel checkbox — wraps shadcn Checkbox with Flex shape. */
export const Checkbox = ({
  checked,
  indeterminate = false,
  onChange,
  label,
  disabled,
  className,
  ...props
}: CheckboxProps) => {
  return (
    <UiCheckbox
      checked={indeterminate && !checked ? "indeterminate" : checked}
      onCheckedChange={(value) => onChange(value === true)}
      disabled={disabled}
      aria-label={label}
      title={label}
      className={cn(
        "size-3.5 rounded-full shadow-none after:hidden",
        "data-checked:border-accent data-checked:bg-accent data-checked:text-accent-text",
        "data-[state=indeterminate]:border-accent data-[state=indeterminate]:bg-accent data-[state=indeterminate]:text-accent-text",
        "data-unchecked:border-stroke-2 data-unchecked:bg-transparent",
        className,
      )}
      {...props}
    />
  )
}

import type { ButtonHTMLAttributes } from "react"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"

type ToggleProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onChange"> & {
  checked: boolean
  onChange: (checked: boolean) => void
  label: string
  small?: boolean
}

/** Settings switch — shadcn Switch with green ON track (`--color-switch-on`). */
export const Toggle = ({
  checked,
  onChange,
  label,
  small = false,
  disabled,
  className,
  ...props
}: ToggleProps) => {
  return (
    <Switch
      checked={checked}
      onCheckedChange={onChange}
      disabled={disabled}
      size={small ? "sm" : "default"}
      aria-label={label}
      title={label}
      className={cn(
        "data-checked:bg-switch-on data-unchecked:bg-fill-2",
        checked && "shadow-[0_0_0_1px_var(--color-border)]",
        className,
      )}
      {...props}
    />
  )
}

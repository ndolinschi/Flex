import type { ButtonHTMLAttributes } from "react"
import { cn } from "../../lib/utils"

type ToggleProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onChange"> & {
  checked: boolean
  onChange: (checked: boolean) => void
  label: string
  small?: boolean
}

/** Settings switch (design-map/07-settings.md §5, `.solid-switch`). Track
 * 30x18 / radius 9, knob 14x14 inset 2px, .2s ease on both — ON is GREEN
 * (`--color-switch-on`), not the accent blue used elsewhere in the app. */
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
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative inline-flex shrink-0 items-center rounded-full transition-colors duration-[0.2s] ease-[ease]",
        small ? "h-3.5 w-6" : "h-[18px] w-[30px]",
        checked ? "bg-switch-on" : "bg-fill-2",
        checked && "shadow-[0_0_0_1px_var(--color-border)]",
        disabled ? "cursor-not-allowed opacity-50" : "cursor-pointer",
        className,
      )}
      {...props}
    >
      <span
        aria-hidden
        className={cn(
          "absolute rounded-full bg-white transition-transform duration-[0.2s] ease-[ease]",
          small ? "left-0.5 h-2.5 w-2.5" : "left-0.5 h-3.5 w-3.5",
          checked
            ? small
              ? "translate-x-2.5"
              : "translate-x-3"
            : "translate-x-0",
        )}
      />
    </button>
  )
}

import { forwardRef, type InputHTMLAttributes } from "react"
import { cn } from "../../lib/utils"

type TextInputProps = InputHTMLAttributes<HTMLInputElement>

export const TextInput = forwardRef<HTMLInputElement, TextInputProps>(
  ({ className, ...props }, ref) => {
    return (
      <input
        ref={ref}
        className={cn(
          "h-8 w-full rounded-md border border-border bg-surface px-2.5 text-sm text-ink",
          "placeholder:text-ink-faint",
          "focus:border-accent focus:outline-none focus:[box-shadow:0_0_0_1px_var(--color-accent)]",
          "disabled:cursor-not-allowed disabled:opacity-50",
          className,
        )}
        {...props}
      />
    )
  },
)

TextInput.displayName = "TextInput"

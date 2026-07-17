import { forwardRef, type InputHTMLAttributes } from "react"
import { Input } from "@/components/ui/input"
import { cn } from "@/lib/utils"

type TextInputProps = InputHTMLAttributes<HTMLInputElement>

/** Compat alias over shadcn `Input` — keeps `TextInput` call-site name. */
export const TextInput = forwardRef<HTMLInputElement, TextInputProps>(
  ({ className, ...props }, ref) => {
    return (
      <Input
        ref={ref}
        className={cn(
          "rounded-md bg-surface text-ink placeholder:text-ink-faint",
          className,
        )}
        {...props}
      />
    )
  },
)

TextInput.displayName = "TextInput"

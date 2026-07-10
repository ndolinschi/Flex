import { forwardRef, type TextareaHTMLAttributes } from "react"
import { cn } from "../../lib/utils"

type TextAreaProps = TextareaHTMLAttributes<HTMLTextAreaElement>

export const TextArea = forwardRef<HTMLTextAreaElement, TextAreaProps>(
  ({ className, ...props }, ref) => {
    return (
      <textarea
        ref={ref}
        className={cn(
          "w-full resize-none rounded-md border border-border bg-surface px-2.5 py-1.5 text-sm text-ink",
          "placeholder:text-ink-faint leading-normal",
          "focus:border-accent focus:outline-none focus:[box-shadow:0_0_0_1px_var(--color-accent)]",
          "disabled:cursor-not-allowed disabled:opacity-50",
          className,
        )}
        {...props}
      />
    )
  },
)

TextArea.displayName = "TextArea"

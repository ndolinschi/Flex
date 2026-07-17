import { forwardRef, type TextareaHTMLAttributes } from "react"
import { Textarea } from "@/components/ui/textarea"
import { cn } from "@/lib/utils"

type TextAreaProps = TextareaHTMLAttributes<HTMLTextAreaElement>

/** Compat alias over shadcn `Textarea` — keeps `TextArea` call-site name. */
export const TextArea = forwardRef<HTMLTextAreaElement, TextAreaProps>(
  ({ className, ...props }, ref) => {
    return (
      <Textarea
        ref={ref}
        className={cn(
          "min-h-0 resize-none rounded-md bg-surface px-2.5 py-1.5 text-ink",
          "placeholder:text-ink-faint leading-normal",
          className,
        )}
        {...props}
      />
    )
  },
)

TextArea.displayName = "TextArea"

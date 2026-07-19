import * as React from "react"

import { cn } from "@/lib/utils"

/** Multi-line text control — Flex chrome (surface + border), not transparent
 * primary defaults. Composer draft stays a specialized raw textarea. */
function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(
        "flex field-sizing-content min-h-16 w-full rounded-md border border-border bg-surface px-2.5 py-1.5 text-sm leading-normal text-ink transition-colors outline-none",
        "placeholder:text-ink-faint",
        "focus-visible:border-stroke-2 focus-visible:ring-3 focus-visible:ring-ring/50",
        "disabled:cursor-not-allowed disabled:opacity-50",
        "aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20",
        "resize-none",
        className,
      )}
      {...props}
    />
  )
}

export { Textarea }

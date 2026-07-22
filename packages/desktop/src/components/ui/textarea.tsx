import * as React from "react"

import { cn } from "@/lib/utils"

/** Multi-line text control — Flex chrome (surface + border), not transparent
 * primary defaults. Composer draft stays a specialized raw textarea. */
function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(
        "flex field-sizing-content min-h-16 w-full rounded-md border border-stroke-3 bg-elevated px-2.5 py-1.5 text-sm leading-normal text-ink outline-none",
        /* Border/color only — exclude box-shadow so focus ring is instant. */
        "transition-[color,background-color,border-color] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
        "placeholder:text-ink-faint",
        "focus-visible:border-stroke-2 focus-visible:ring-1 focus-visible:ring-stroke-2",
        "disabled:cursor-not-allowed disabled:opacity-50",
        "aria-invalid:border-destructive aria-invalid:ring-1 aria-invalid:ring-destructive/30",
        "resize-none",
        className,
      )}
      {...props}
    />
  )
}

export { Textarea }

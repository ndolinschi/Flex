import * as React from "react"
import { Input as InputPrimitive } from "@base-ui/react/input"

import { cn } from "@/lib/utils"

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <InputPrimitive
      type={type}
      data-slot="input"
      className={cn(
        // Hairline stroke-3, elevated fill, neutral stroke-2 focus (no accent glow).
        // Phase 6: elevated fill, stroke-3 idle → stroke-2 focus, no accent glow.
        "h-8 w-full min-w-0 rounded-md border border-stroke-3 bg-elevated px-2.5 py-1 text-base text-ink outline-none",
        "transition-[color,background-color,border-color] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
        "file:inline-flex file:h-6 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-ink",
        "placeholder:text-ink-faint",
        "focus-visible:border-stroke-2 focus-visible:ring-1 focus-visible:ring-stroke-2",
        "disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-fill-5 disabled:opacity-50",
        "aria-invalid:border-danger focus-visible:aria-invalid:ring-danger/30",
        className
      )}
      {...props}
    />
  )
}

export { Input }

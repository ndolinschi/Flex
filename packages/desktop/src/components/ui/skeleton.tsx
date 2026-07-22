import { cn } from "@/lib/utils"

/** Whisper fill placeholder — soft pulse on surface-muted, opacity dampened
 * so loading never reads as a bright shimmer slab (DESIGN.md States). */
function Skeleton({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="skeleton"
      className={cn(
        "animate-pulse rounded-md bg-surface-muted opacity-70",
        className,
      )}
      {...props}
    />
  )
}

export { Skeleton }

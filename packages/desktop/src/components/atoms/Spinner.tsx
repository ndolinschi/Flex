import { cn } from "@/lib/utils"
import { Spinner as SpinnerPrimitive } from "@/components/ui/spinner"

type SpinnerSize = "sm" | "md" | "lg"

type SpinnerProps = {
  size?: SpinnerSize
  className?: string
}

const sizeMap: Record<SpinnerSize, string> = {
  sm: "size-3.5",
  md: "size-4",
  lg: "size-7",
}

/** Indeterminate loading — muted ink, not a bright accent spinner. */
export const Spinner = ({ size = "md", className }: SpinnerProps) => (
  <SpinnerPrimitive className={cn(sizeMap[size], "text-ink-muted", className)} />
)

import { Spinner as UiSpinner } from "@/components/ui/spinner"
import { cn } from "@/lib/utils"

type SpinnerSize = "sm" | "md" | "lg"

type SpinnerProps = {
  size?: SpinnerSize
  className?: string
}

const sizeMap: Record<SpinnerSize, string> = {
  sm: "size-3.5",
  md: "size-5",
  lg: "size-7",
}

/** Compat size prop over shadcn `Spinner` (Loader2). */
export const Spinner = ({ size = "md", className }: SpinnerProps) => {
  return <UiSpinner className={cn(sizeMap[size], "text-accent", className)} />
}

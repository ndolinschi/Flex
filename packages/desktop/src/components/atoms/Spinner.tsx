import { cn } from "../../lib/utils"

type SpinnerSize = "sm" | "md" | "lg"

type SpinnerProps = {
  size?: SpinnerSize
  className?: string
}

const sizeMap: Record<SpinnerSize, string> = {
  sm: "h-3.5 w-3.5 border",
  md: "h-5 w-5 border-2",
  lg: "h-7 w-7 border-2",
}

export const Spinner = ({ size = "md", className }: SpinnerProps) => {
  return (
    <span
      role="status"
      aria-label="Loading"
      className={cn(
        "inline-block animate-spin rounded-full border-accent/30 border-t-accent",
        sizeMap[size],
        className,
      )}
    />
  )
}

import { cn } from "../../lib/utils"

type StreamingCaretProps = {
  className?: string
}

export const StreamingCaret = ({ className }: StreamingCaretProps) => {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "ml-0.5 inline-block h-4 w-0.5 animate-pulse bg-accent align-middle",
        className,
      )}
    />
  )
}

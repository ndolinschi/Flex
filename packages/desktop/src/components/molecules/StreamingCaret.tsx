import { cn } from "../../lib/utils"

type StreamingCaretProps = {
  className?: string
}

/** Thin streaming caret — subtle pulse, never a bright block cursor. */
export const StreamingCaret = ({ className }: StreamingCaretProps) => {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "ml-0.5 inline-block h-3.5 w-px animate-pulse bg-ink-muted align-middle opacity-70",
        className,
      )}
    />
  )
}

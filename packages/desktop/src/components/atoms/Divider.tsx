import { cn } from "../../lib/utils"

type DividerProps = {
  className?: string
  label?: string
}

export const Divider = ({ className, label }: DividerProps) => {
  if (label) {
    return (
      <div className={cn("flex items-center gap-3", className)} role="separator">
        <div className="h-px flex-1 bg-border" />
        <span className="text-xs text-ink-faint">{label}</span>
        <div className="h-px flex-1 bg-border" />
      </div>
    )
  }

  return (
    <hr className={cn("border-0 border-t border-border", className)} />
  )
}

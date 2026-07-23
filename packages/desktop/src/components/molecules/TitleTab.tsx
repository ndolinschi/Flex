import { cn } from "../../lib/utils"

type TitleTabProps = {
  title: string
  className?: string
}

export const TitleTab = ({ title, className }: TitleTabProps) => {
  return (
    <div
      className={cn(
        "flex min-w-0 items-center rounded-sm px-1.5 py-0.5",
        "text-sm text-ink-muted",
        className,
      )}
    >
      <span className="min-w-0 max-w-56 truncate">{title}</span>
    </div>
  )
}

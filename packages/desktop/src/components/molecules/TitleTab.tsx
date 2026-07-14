import { cn } from "../../lib/utils"

type TitleTabProps = {
  title: string
  className?: string
}

/** Chat title for the top bar — plain label, not a second panel control.
 * Right-panel open/close lives solely on AppHeader's PanelRight (⌘J). */
export const TitleTab = ({ title, className }: TitleTabProps) => {
  return (
    <div
      className={cn(
        "flex min-w-0 items-center rounded-sm px-1.5 py-0.5",
        "text-sm text-ink-secondary opacity-70",
        className,
      )}
    >
      <span className="min-w-0 max-w-56 truncate">{title}</span>
    </div>
  )
}

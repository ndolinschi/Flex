import type { ReactNode } from "react"
import { cn } from "../../lib/utils"

type ChatThreadHeaderProps = {
  /** Agent / session title — production text-base font-medium truncate. */
  title: string
  /** Optional trailing cluster (repo chip, expand, panel toggle). */
  trailing?: ReactNode
  className?: string
}

/**
 * Production chat thread header (Agents Web 2026-07-23 7：56):
 * `h-[40px] pl-3 pr-2` · title left · actions right.
 * Desktop keeps WindowTitleBar for window chrome; this is the in-pane
 * conversation title row above the timeline.
 */
export const ChatThreadHeader = ({
  title,
  trailing,
  className,
}: ChatThreadHeaderProps) => {
  return (
    <header
      className={cn(
        "chat-thread-header flex h-[var(--chat-header-height)] w-full shrink-0 items-center justify-between gap-3 pl-3 pr-2",
        className,
      )}
      data-slot="chat-thread-header"
    >
      <div className="flex min-w-0 flex-1 items-center gap-2">
        {title.trim() ? (
          <h1 className="chat-thread-title" title={title}>
            {title}
          </h1>
        ) : null}
      </div>
      {trailing ? (
        <div className="flex shrink-0 flex-nowrap items-center gap-1.5">
          {trailing}
        </div>
      ) : null}
    </header>
  )
}

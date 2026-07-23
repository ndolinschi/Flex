import type { ReactNode } from "react"
import { cn } from "../../lib/utils"

type ChatThreadHeaderProps = {
  title: string
  trailing?: ReactNode
  className?: string
}

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

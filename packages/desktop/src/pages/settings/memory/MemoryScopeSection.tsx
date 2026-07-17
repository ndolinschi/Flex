import { useState } from "react"
import { ChevronDown } from "@/components/icons"
import { Collapsible } from "../../../components/molecules"
import type { MemoryEntryDto } from "../../../lib/types"
import { cn } from "../../../lib/utils"
import type { MemoryScope } from "./constants"
import { MemoryRow } from "./MemoryRow"

/** One collapsible "Global" or per-project section, styled like a Settings
 * section header (title + entry count), holding a list of `MemoryRow`s
 * scoped to whichever get/remove/expiry functions the caller passes in. */
export const MemoryScopeSection = ({
  title,
  hint,
  memories,
  scope,
  defaultOpen = true,
}: {
  title: string
  hint?: string
  memories: MemoryEntryDto[]
  scope: MemoryScope
  defaultOpen?: boolean
}) => {
  const [open, setOpen] = useState(defaultOpen)

  return (
    <section>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="mb-2 flex w-full items-center gap-2 px-3.5 text-left"
      >
        <ChevronDown
          className={cn(
            "h-3 w-3 shrink-0 text-ink-muted transition-transform duration-[var(--duration-fast)]",
            !open && "-rotate-90",
          )}
          aria-hidden
        />
        <h2 className="text-sm leading-4 text-ink-secondary">{title}</h2>
        <span className="text-xs text-ink-faint">{memories.length}</span>
        {hint ? (
          <span className="truncate text-xs text-ink-faint">{hint}</span>
        ) : null}
      </button>
      <Collapsible open={open}>
        <div className="flex flex-col gap-1.5">
          {memories.map((memory) => (
            <MemoryRow key={memory.id} memory={memory} scope={scope} />
          ))}
        </div>
      </Collapsible>
    </section>
  )
}

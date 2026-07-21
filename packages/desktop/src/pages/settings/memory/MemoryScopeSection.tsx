import { ChevronDown } from "lucide-react"
import type { MemoryEntryDto } from "../../../lib/types"
import { cn } from "../../../lib/utils"
import type { MemoryScope } from "./constants"
import { MemoryRow } from "./MemoryRow"
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion"

/** One accordion "Global" or per-project section, styled like a Settings
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
  return (
    <Accordion
      multiple
      defaultValue={defaultOpen ? ["scope"] : []}
      className="w-full"
    >
      <AccordionItem value="scope" className="border-0">
        <AccordionTrigger
          className={cn(
            "mb-2 justify-start gap-2 rounded-none px-3.5 py-0 font-normal hover:no-underline",
            "**:data-[slot=accordion-trigger-icon]:hidden",
          )}
        >
          <ChevronDown
            className={cn(
              "h-3 w-3 shrink-0 text-ink-muted transition-transform duration-[var(--duration-fast)]",
              "-rotate-90 group-aria-expanded/accordion-trigger:rotate-0",
            )}
            aria-hidden
          />
          <span className="text-sm leading-4 text-ink-secondary">{title}</span>
          <span className="text-xs text-ink-faint">{memories.length}</span>
          {hint ? (
            <span className="truncate text-xs text-ink-faint">{hint}</span>
          ) : null}
        </AccordionTrigger>
        <AccordionContent className="pb-0">
          <div className="flex flex-col gap-1.5">
            {memories.map((memory) => (
              <MemoryRow key={memory.id} memory={memory} scope={scope} />
            ))}
          </div>
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  )
}

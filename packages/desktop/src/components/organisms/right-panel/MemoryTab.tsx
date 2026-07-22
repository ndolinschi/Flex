import { Brain } from "lucide-react"
import { MemoryContent } from "../../../pages/settings/memory/MemoryContent"
import { ScrollArea } from "@/components/ui/scroll-area"

/** Right-panel Memory tab — same durable-notes UI as Settings → Memory.
 * Gated by `MEMORY_TAB_ENABLED` at the tab-strip / open sites; this body
 * only mounts when the tab is open. Works empty (no memories yet).
 * Chrome matches Browser/PlanList: 30px `--header-height` row + `px-2.5`. */
export const MemoryTab = () => {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1.5 px-2.5">
        <Brain className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
        <span className="min-w-0 flex-1 truncate text-sm text-ink">Memory</span>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="px-2.5 py-3">
          <MemoryContent />
        </div>
      </ScrollArea>
    </div>
  )
}

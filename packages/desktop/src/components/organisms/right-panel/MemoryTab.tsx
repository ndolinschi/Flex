import { ScrollArea } from "../../atoms"
import { MemoryContent } from "../../../pages/settings/memory/MemoryContent"

/** Right-panel Memory tab — same durable-notes UI as Settings → Memory.
 * Gated by `MEMORY_TAB_ENABLED` at the tab-strip / open sites; this body
 * only mounts when the tab is open. Works empty (no memories yet). */
export const MemoryTab = () => {
  return (
    <ScrollArea className="min-h-0 flex-1 px-2.5 py-3">
      <MemoryContent />
    </ScrollArea>
  )
}

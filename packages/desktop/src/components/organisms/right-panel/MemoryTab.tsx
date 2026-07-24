import { Brain } from "lucide-react"
import { MemoryContent } from "../../../pages/settings/memory/MemoryContent"
import { PanelToolbar, PanelToolbarTitle } from "../../molecules"
import { ScrollArea } from "@/components/ui/scroll-area"

export const MemoryTab = () => {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <PanelToolbar aria-label="Memory">
        <PanelToolbarTitle icon={<Brain aria-hidden />}>Memory</PanelToolbarTitle>
      </PanelToolbar>
      <ScrollArea className="min-h-0 flex-1">
        <div className="px-2.5 py-2">
          <MemoryContent compact />
        </div>
      </ScrollArea>
    </div>
  )
}

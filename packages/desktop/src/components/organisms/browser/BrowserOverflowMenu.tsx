import {
  Camera,
  Copy,
  Eraser,
  History,
  MoreHorizontal,
  RotateCw,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

export type BrowserOverflowMenuProps = {
  open: boolean
  onOpenChange: (open: boolean) => void
  browserStarted: boolean
  showLiveContent: boolean
  browserUrl: string
  onScreenshot: () => void | Promise<void>
  onHardReload: () => void
  onCopyUrl: () => void | Promise<void>
  onClearHistory: () => void | Promise<void>
  onClearData: () => void | Promise<void>
}

/** Browser "…" overflow actions menu. */
export const BrowserOverflowMenu = ({
  open,
  onOpenChange,
  browserStarted,
  showLiveContent,
  browserUrl,
  onScreenshot,
  onHardReload,
  onCopyUrl,
  onClearHistory,
  onClearData,
}: BrowserOverflowMenuProps) => {
  return (
    <DropdownMenu open={open} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger
        render={
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label="More browser actions"
            className="size-6 text-muted-foreground hover:text-foreground aria-expanded:bg-accent aria-expanded:text-foreground"
          />
        }
      >
        <MoreHorizontal className="size-3.5" aria-hidden />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" sideOffset={4} className="w-56">
        <DropdownMenuGroup>
          <DropdownMenuItem
            disabled={!showLiveContent}
            onClick={() => void onScreenshot()}
          >
            <Camera />
            Take Screenshot
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={!showLiveContent}
            onClick={onHardReload}
          >
            <RotateCw />
            Hard Reload
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={!browserUrl}
            onClick={() => void onCopyUrl()}
          >
            <Copy />
            Copy Current URL
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuItem
            disabled={!browserStarted}
            onClick={() => void onClearHistory()}
          >
            <History />
            Clear Browsing History
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={!showLiveContent}
            onClick={() => void onClearData()}
          >
            <Eraser />
            Clear Browsing Data
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

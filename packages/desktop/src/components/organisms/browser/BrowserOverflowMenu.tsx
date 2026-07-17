import {
  Camera,
  Copy,
  Eraser,
  History,
  MoreHorizontal,
  RotateCw,
} from "lucide-react"
import { IconButton } from "../../atoms"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { cn } from "@/lib/utils"

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

/** Browser "…" overflow actions — shadcn DropdownMenu. */
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
      <DropdownMenuTrigger asChild>
        <IconButton
          label="More browser actions"
          className={cn("h-6 w-6", open && "bg-fill-3 text-ink")}
        >
          <MoreHorizontal className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      </DropdownMenuTrigger>

      <DropdownMenuContent
        align="end"
        sideOffset={4}
        className="w-56 min-w-56 rounded-lg border border-stroke-2 bg-panel p-0.5 shadow-lg ring-0"
      >
        <DropdownMenuGroup>
          <DropdownMenuItem
            className="gap-2 px-2.5 py-1.5"
            disabled={!showLiveContent}
            onSelect={() => void onScreenshot()}
          >
            <Camera className="size-3.5 text-icon-3" aria-hidden />
            Take Screenshot
          </DropdownMenuItem>
          <DropdownMenuItem
            className="gap-2 px-2.5 py-1.5"
            disabled={!showLiveContent}
            onSelect={onHardReload}
          >
            <RotateCw className="size-3.5 text-icon-3" aria-hidden />
            Hard Reload
          </DropdownMenuItem>
          <DropdownMenuItem
            className="gap-2 px-2.5 py-1.5"
            disabled={!browserUrl}
            onSelect={() => void onCopyUrl()}
          >
            <Copy className="size-3.5 text-icon-3" aria-hidden />
            Copy Current URL
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator className="mx-2 bg-stroke-3" />
        <DropdownMenuGroup>
          <DropdownMenuItem
            className="gap-2 px-2.5 py-1.5"
            disabled={!browserStarted}
            onSelect={() => void onClearHistory()}
          >
            <History className="size-3.5 text-icon-3" aria-hidden />
            Clear Browsing History
          </DropdownMenuItem>
          <DropdownMenuItem
            className="gap-2 px-2.5 py-1.5"
            disabled={!showLiveContent}
            onSelect={() => void onClearData()}
          >
            <Eraser className="size-3.5 text-icon-3" aria-hidden />
            Clear Browsing Data
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

import { useState } from "react"
import { FileIcon, ImageIcon, Plus } from "@/components/icons"
import { cn } from "../../lib/utils"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

type PlusMenuProps = {
  onAttachFile: () => void
  onAttachImage: () => void
  disabled?: boolean
}

/** Composer “+” menu — attach file/image via shadcn DropdownMenu. */
export const PlusMenu = ({
  onAttachFile,
  onAttachImage,
  disabled = false,
}: PlusMenuProps) => {
  const [open, setOpen] = useState(false)

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label="Add context"
          title="Add context"
          disabled={disabled}
          className={cn(
            "inline-flex h-6 w-6 items-center justify-center rounded-full",
            "text-icon-2 opacity-50 transition-[opacity,background-color,color] duration-[var(--duration-fast)]",
            "hover:bg-fill-3 hover:opacity-80 disabled:opacity-30",
            open && "bg-fill-3 opacity-80 text-ink",
          )}
        >
          <Plus className="h-3.5 w-3.5" aria-hidden />
        </button>
      </DropdownMenuTrigger>

      <DropdownMenuContent
        side="top"
        align="start"
        sideOffset={6}
        className="w-56 min-w-56 rounded-md border-0 bg-panel p-0 shadow-[var(--shadow-popover)] ring-0"
      >
        <DropdownMenuLabel className="border-b border-stroke-3 px-2.5 py-1.5 text-xs font-normal text-ink-faint">
          Add agents, context, tools…
        </DropdownMenuLabel>
        <DropdownMenuGroup className="py-1">
          <DropdownMenuItem
            className="gap-2 px-2.5 py-1.5"
            onSelect={() => onAttachFile()}
          >
            <FileIcon className="size-3.5" aria-hidden />
            Attach file
          </DropdownMenuItem>
          <DropdownMenuItem
            className="gap-2 px-2.5 py-1.5"
            onSelect={() => onAttachImage()}
          >
            <ImageIcon className="size-3.5" aria-hidden />
            Attach image
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

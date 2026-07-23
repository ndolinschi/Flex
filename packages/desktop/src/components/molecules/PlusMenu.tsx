import { useState } from "react"
import { FileIcon, ImageIcon, PlusIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { cn } from "@/lib/utils"

type PlusMenuProps = {
  onAttachFile: () => void
  onAttachImage: () => void
  disabled?: boolean
}

export const PlusMenu = ({
  onAttachFile,
  onAttachImage,
  disabled = false,
}: PlusMenuProps) => {
  const [open, setOpen] = useState(false)

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger
        disabled={disabled}
        render={
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label="Add context"
            title={open ? undefined : "Add context"}
            className={cn(
              "size-6 rounded-full bg-fill-4 text-icon-2 opacity-50",
              "transition-[opacity,background-color,color,transform] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
              "hover:bg-fill-2 hover:text-icon-1 hover:opacity-80",
              "active:translate-y-px active:opacity-80 motion-reduce:active:translate-y-0",
              "aria-expanded:bg-fill-2 aria-expanded:text-icon-1 aria-expanded:opacity-80",
            )}
          />
        }
      >
        <PlusIcon className="size-3.5" strokeWidth={1.5} aria-hidden />
      </DropdownMenuTrigger>
      {open ? (
        <DropdownMenuContent side="top" align="start" className="w-56">
          <DropdownMenuGroup>
            <DropdownMenuLabel>Add agents, context, tools…</DropdownMenuLabel>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={onAttachFile}>
              <FileIcon />
              Attach file
            </DropdownMenuItem>
            <DropdownMenuItem onClick={onAttachImage}>
              <ImageIcon />
              Attach image
            </DropdownMenuItem>
          </DropdownMenuGroup>
        </DropdownMenuContent>
      ) : null}
    </DropdownMenu>
  )
}

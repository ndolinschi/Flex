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
            title="Add context"
            className={cn(
              // Quiet + circle — matches Bypass/Send hit target, not a filled chip.
              "size-6 rounded-full text-ink-muted opacity-60",
              "transition-[opacity,background-color,color,transform] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
              "hover:bg-fill-4 hover:text-ink hover:opacity-100",
              "active:translate-y-px active:opacity-100",
              "aria-expanded:bg-fill-4 aria-expanded:text-ink aria-expanded:opacity-100",
            )}
          />
        }
      >
        <PlusIcon />
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

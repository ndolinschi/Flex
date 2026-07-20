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
              "size-6 rounded-full text-muted-foreground opacity-50 hover:bg-accent hover:opacity-80",
              "aria-expanded:bg-accent aria-expanded:text-foreground aria-expanded:opacity-80",
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

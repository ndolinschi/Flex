import { useRef, useState } from "react"
import { FileIcon, ImageIcon, PlusIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { PopoverItem, PopoverTray } from "./PopoverTray"

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
  const rootRef = useRef<HTMLDivElement>(null)

  const handleClose = () => setOpen(false)

  return (
    <div ref={rootRef} className="relative">
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        aria-label="Add context"
        title="Add context"
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "size-6 rounded-full text-icon-2 opacity-50 hover:bg-fill-3 hover:opacity-80",
          open && "bg-fill-3 text-ink opacity-80",
        )}
      >
        <PlusIcon />
      </Button>

      <PopoverTray
        open={open}
        onClose={handleClose}
        anchorRef={rootRef}
        placement="above"
        role="menu"
        aria-label="Add agents, context, tools"
        className="left-0 w-56"
      >
        <p className="border-b border-stroke-3 px-2.5 py-1.5 text-xs text-ink-faint">
          Add agents, context, tools…
        </p>
        <PopoverItem
          role="menuitem"
          onClick={() => {
            handleClose()
            onAttachFile()
          }}
        >
          <FileIcon className="h-3.5 w-3.5" aria-hidden />
          Attach file
        </PopoverItem>
        <PopoverItem
          role="menuitem"
          onClick={() => {
            handleClose()
            onAttachImage()
          }}
        >
          <ImageIcon className="h-3.5 w-3.5" aria-hidden />
          Attach image
        </PopoverItem>
      </PopoverTray>
    </div>
  )
}

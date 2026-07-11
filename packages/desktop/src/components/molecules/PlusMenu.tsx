import { useRef, useState } from "react"
import { FileIcon, ImageIcon, Plus } from "lucide-react"
import { cn } from "../../lib/utils"
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
      <button
        type="button"
        aria-label="Add context"
        title="Add context"
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "inline-flex h-6 w-6 items-center justify-center rounded-full",
          "text-icon-2 opacity-50 transition-[opacity,background-color,color] duration-[var(--duration-fast)]",
          "hover:bg-fill-3 hover:opacity-80 disabled:opacity-30",
          open && "bg-fill-3 opacity-80 text-ink",
        )}
      >
        <Plus className="h-3.5 w-3.5" aria-hidden />
      </button>

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

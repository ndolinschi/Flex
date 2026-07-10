import { useRef, useState } from "react"
import {
  FileIcon,
  ImageIcon,
  ListTodo,
  MessageCircle,
  Plus,
} from "lucide-react"
import type { ComposerMode } from "../../lib/types"
import { cn } from "../../lib/utils"
import { PopoverItem, PopoverTray } from "./PopoverTray"

type PlusMenuProps = {
  onAttachFile: () => void
  onAttachImage: () => void
  onSetMode?: (mode: ComposerMode) => void
  disabled?: boolean
}

export const PlusMenu = ({
  onAttachFile,
  onAttachImage,
  onSetMode,
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
        {onSetMode ? (
          <>
            <PopoverItem
              role="menuitem"
              onClick={() => {
                handleClose()
                onSetMode("plan")
              }}
            >
              <ListTodo className="h-3.5 w-3.5 text-yellow" aria-hidden />
              Plan
            </PopoverItem>
            <PopoverItem
              role="menuitem"
              onClick={() => {
                handleClose()
                onSetMode("ask")
              }}
            >
              <MessageCircle className="h-3.5 w-3.5 text-cyan" aria-hidden />
              Ask
            </PopoverItem>
            <div className="mx-2 my-0.5 border-t border-stroke-3" />
          </>
        ) : null}
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

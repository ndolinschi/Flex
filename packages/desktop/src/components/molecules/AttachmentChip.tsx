import { FileIcon, Folder, ImageIcon, MousePointer2, Palette, X } from "lucide-react"
import type { ComposerAttachment } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"

type AttachmentChipProps = {
  attachment: ComposerAttachment
  onRemove: (id: string) => void
}

/** context pill: icon + name + inline remove. */
export const AttachmentChip = ({ attachment, onRemove }: AttachmentChipProps) => {
  const Icon =
    attachment.kind === "image"
      ? ImageIcon
      : attachment.kind === "dom"
        ? MousePointer2
        : attachment.kind === "component-style"
          ? Palette
          : attachment.kind === "directory"
            ? Folder
            : FileIcon

  return (
    <span
      className={cn(
        "group/chip inline-flex h-5 max-w-[12rem] items-center gap-1 rounded-[4px]",
        "border border-stroke-3 bg-fill-3 px-1 text-sm text-ink-secondary",
        "transition-colors duration-[var(--duration-fast)] hover:border-stroke-2 hover:bg-fill-2",
      )}
    >
      <Icon className="h-3 w-3 shrink-0 text-icon-3" aria-hidden />
      <span className="truncate">{attachment.name}</span>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label={`Remove ${attachment.name}`}
        onClick={() => onRemove(attachment.id)}
        className="shrink-0 text-icon-3 hover:bg-fill-2 hover:text-ink"
      >
        <X className="h-2.5 w-2.5" aria-hidden />
      </Button>
    </span>
  )
}

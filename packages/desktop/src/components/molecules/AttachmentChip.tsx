import { FileIcon, Folder, ImageIcon, MousePointer2, Palette, X } from "@/components/icons"
import type { ComposerAttachment } from "../../lib/types"
import { cn } from "../../lib/utils"
import {
  Attachment,
  AttachmentAction,
  AttachmentActions,
  AttachmentContent,
  AttachmentMedia,
  AttachmentTitle,
} from "@/components/ui/attachment"

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
    <Attachment
      state="done"
      size="xs"
      orientation="horizontal"
      className={cn(
        "h-5 max-w-[12rem] min-w-0 items-center gap-1 rounded-[4px]",
        "border-stroke-3 bg-fill-3 text-ink-secondary",
        "hover:border-stroke-2 hover:bg-fill-2",
        "has-[>a,>button]:hover:bg-fill-2",
      )}
    >
      <AttachmentMedia
        variant="icon"
        className="aspect-auto size-auto w-auto rounded-none bg-transparent p-0 text-icon-3 [&_svg]:size-3"
      >
        <Icon aria-hidden />
      </AttachmentMedia>
      <AttachmentContent className="min-w-0 px-0 py-0">
        <AttachmentTitle className="text-sm font-normal text-ink-secondary">
          {attachment.name}
        </AttachmentTitle>
      </AttachmentContent>
      <AttachmentActions>
        <AttachmentAction
          type="button"
          size="icon-xs"
          variant="ghost"
          aria-label={`Remove ${attachment.name}`}
          onClick={() => onRemove(attachment.id)}
          className={cn(
            "size-4 rounded text-icon-3 hover:bg-fill-2 hover:text-ink",
          )}
        >
          <X className="size-2.5" aria-hidden />
        </AttachmentAction>
      </AttachmentActions>
    </Attachment>
  )
}

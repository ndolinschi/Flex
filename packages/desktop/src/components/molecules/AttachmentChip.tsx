import { FileIcon, Folder, ImageIcon, MousePointer2, Palette, X } from "lucide-react"
import type { ComposerAttachment } from "../../lib/types"
import {
  Attachment,
  AttachmentActions,
  AttachmentAction,
  AttachmentContent,
  AttachmentMedia,
  AttachmentTitle,
} from "@/components/ui/attachment"

type AttachmentChipProps = {
  attachment: ComposerAttachment
  onRemove: (id: string) => void
}

/** Pending attachment chip in the Composer input strip. */
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
      size="xs"
      state="done"
      orientation="horizontal"
      // Composer chips are compact pills (Cursor density), not mini-cards.
      // Override kit `min-w-40` / card surface for the toolbar strip.
      className="min-w-0 max-w-[12rem] gap-1 rounded-md border-stroke-3 bg-fill-3 py-0 text-xs text-ink-secondary shadow-none"
    >
      <AttachmentMedia
        variant="icon"
        className="size-4 w-4 rounded-sm bg-transparent text-ink-muted [&_svg:not([class*='size-'])]:size-3"
      >
        <Icon aria-hidden />
      </AttachmentMedia>
      <AttachmentContent className="min-w-0 py-0.5">
        <AttachmentTitle className="truncate font-normal text-ink-secondary">
          {attachment.name}
        </AttachmentTitle>
      </AttachmentContent>
      <AttachmentActions>
        <AttachmentAction
          aria-label={`Remove ${attachment.name}`}
          onClick={() => onRemove(attachment.id)}
          className="size-4 text-ink-faint hover:bg-fill-4 hover:text-ink-muted"
        >
          <X className="size-3" aria-hidden />
        </AttachmentAction>
      </AttachmentActions>
    </Attachment>
  )
}

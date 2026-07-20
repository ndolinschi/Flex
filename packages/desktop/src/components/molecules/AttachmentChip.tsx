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
    <Attachment size="xs" state="done" orientation="horizontal">
      <AttachmentMedia variant="icon">
        <Icon aria-hidden />
      </AttachmentMedia>
      <AttachmentContent>
        <AttachmentTitle>{attachment.name}</AttachmentTitle>
      </AttachmentContent>
      <AttachmentActions>
        <AttachmentAction
          aria-label={`Remove ${attachment.name}`}
          onClick={() => onRemove(attachment.id)}
        >
          <X aria-hidden />
        </AttachmentAction>
      </AttachmentActions>
    </Attachment>
  )
}

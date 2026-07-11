import type { ComposerAttachment } from "../../../lib/types"
import { isBrowserPreview } from "../../../lib/browserMock"

export const extForMimeType = (mimeType: string): string => {
  if (mimeType === "image/png") return "png"
  if (mimeType === "image/gif") return "gif"
  if (mimeType === "image/webp") return "webp"
  return "jpg"
}

/** Attach a clipboard/drop image blob. Preview uses object URLs; native returns false. */
export const attachImageBlob = async (
  blob: File | Blob,
  addAttachment: (att: ComposerAttachment) => void,
  suggestedName?: string,
): Promise<boolean> => {
  const name = suggestedName ?? `pasted-${Date.now()}.${extForMimeType(blob.type)}`
  if (isBrowserPreview()) {
    const url = URL.createObjectURL(blob)
    addAttachment({
      id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      path: url,
      kind: "image",
      name,
    })
    return true
  }
  // No native path to persist the blob to disk — see Composer.tsx comment.
  return false
}

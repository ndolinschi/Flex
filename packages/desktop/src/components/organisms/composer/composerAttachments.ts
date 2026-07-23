import type { ComposerAttachment } from "../../../lib/types"
import { isBrowserPreview } from "../../../lib/browserPreview"
import { writeTempBlob } from "../../../lib/tauri"

export const extForMimeType = (mimeType: string): string => {
  if (mimeType === "image/png") return "png"
  if (mimeType === "image/gif") return "gif"
  if (mimeType === "image/webp") return "webp"
  return "jpg"
}

export const attachImageBlob = async (
  blob: File | Blob,
  addAttachment: (att: ComposerAttachment) => void,
  suggestedName?: string,
): Promise<boolean> => {
  if (isBrowserPreview()) return false
  const ext = extForMimeType(blob.type)
  const name = suggestedName ?? `pasted-${Date.now()}.${ext}`
  try {
    const bytes = new Uint8Array(await blob.arrayBuffer())
    const path = await writeTempBlob(bytes, ext)
    addAttachment({
      id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      path,
      kind: "image",
      name,
    })
    return true
  } catch {
    return false
  }
}

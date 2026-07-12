import type { ComposerAttachment } from "../../../lib/types"
import { isBrowserPreview } from "../../../lib/browserMock"
import { writeTempBlob } from "../../../lib/tauri"

export const extForMimeType = (mimeType: string): string => {
  if (mimeType === "image/png") return "png"
  if (mimeType === "image/gif") return "gif"
  if (mimeType === "image/webp") return "webp"
  return "jpg"
}

/** Attach a clipboard/drop image blob.
 *
 * Preview: no real filesystem, so the attachment's `path` is an object URL —
 * good enough for the thumbnail preview, never sent anywhere.
 *
 * Native: the blob only exists in memory (clipboard/drag data), and the
 * engine's attachment contract only understands file paths
 * (`PromptAttachment.path` → `BlobSource::Path`, see
 * `src-tauri/src/commands.rs::build_prompt_input`) — there's no inline/base64
 * attachment path. So the bytes are persisted to a temp file via the
 * `write_temp_blob` command and the attachment's `path` is the real absolute
 * path the engine will read at send time.
 *
 * Returns `false` only on failure (e.g. the native write rejected the blob),
 * so the caller can surface an error instead of silently dropping the paste. */
export const attachImageBlob = async (
  blob: File | Blob,
  addAttachment: (att: ComposerAttachment) => void,
  suggestedName?: string,
): Promise<boolean> => {
  const ext = extForMimeType(blob.type)
  const name = suggestedName ?? `pasted-${Date.now()}.${ext}`
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

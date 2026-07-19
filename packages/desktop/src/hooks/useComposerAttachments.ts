import { useState, type ClipboardEvent, type DragEvent } from "react"
import { open } from "@tauri-apps/plugin-dialog"
import { attachImageBlob } from "../components/organisms/composer/composerAttachments"
import { isBrowserPreview, NATIVE_APP_REQUIRED } from "../lib/browserPreview"
import { toInvokeError } from "../lib/tauri"
import { useAppStore } from "../stores/appStore"

/** Owns the composer's attachment list (paste/drop/file-pick) and surfaces the
 * error string the view should show in the `ErrorBanner`. Attachment state
 * itself lives in the store (shared with send/queue logic in
 * `useComposerSend`), this hook only owns the handlers that mutate it. */
export const useComposerAttachments = () => {
  const attachments = useAppStore((s) => s.attachments)
  const addAttachment = useAppStore((s) => s.addAttachment)
  const removeAttachment = useAppStore((s) => s.removeAttachment)
  const clearAttachments = useAppStore((s) => s.clearAttachments)
  const [error, setError] = useState<string | null>(null)
  /** True while the native OS file dialog is open — dims the + button so a
   * main-thread modal wait does not look like a frozen app. */
  const [picking, setPicking] = useState(false)

  const handlePick = async (kind: "file" | "image") => {
    if (picking) return
    setPicking(true)
    try {
      if (isBrowserPreview()) {
        setError(NATIVE_APP_REQUIRED)
        return
      }
      const selected = await open({
        multiple: true,
        filters:
          kind === "image"
            ? [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp"] }]
            : undefined,
      })
      if (!selected) return
      const paths = Array.isArray(selected) ? selected : [selected]
      for (const path of paths) {
        const name = path.split(/[/\\]/).pop() ?? path
        addAttachment({
          id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          path,
          kind,
          name,
        })
      }
    } catch (err) {
      setError(toInvokeError(err))
    } finally {
      setPicking(false)
    }
  }

  const handlePaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const items = e.clipboardData?.items
    if (!items) return
    const imageItems = Array.from(items).filter((i) => i.type.startsWith("image/"))
    if (imageItems.length === 0) return
    e.preventDefault()
    for (const item of imageItems) {
      const blob = item.getAsFile()
      if (!blob) continue
      void attachImageBlob(blob, addAttachment).then((attached) => {
        if (!attached) {
          setError("Couldn't attach the pasted image.")
        }
      })
    }
  }

  const handleDrop = (e: DragEvent<HTMLTextAreaElement>) => {
    const files = e.dataTransfer?.files
    if (!files || files.length === 0) return
    const images = Array.from(files).filter((f) => f.type.startsWith("image/"))
    if (images.length === 0) return
    e.preventDefault()
    for (const file of images) {
      void attachImageBlob(file, addAttachment, file.name).then((attached) => {
        if (!attached) {
          setError("Couldn't attach the dropped image.")
        }
      })
    }
  }

  return {
    attachments,
    addAttachment,
    removeAttachment,
    clearAttachments,
    error,
    setError,
    picking,
    handlePick,
    handlePaste,
    handleDrop,
  }
}

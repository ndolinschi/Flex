import { useEffect, type MutableRefObject } from "react"

type ShortcutHandlers = {
  onSend?: () => void
  onNewSession?: () => void
  onFocusComposer?: () => void
  onCancel?: () => boolean
  onSearch?: () => void
  onToggleSidebar?: () => void
  onToggleRightPanel?: () => void
  onToggleCommandPalette?: () => void
  onCloseActiveTab?: () => void
}

const isEditableTarget = (target: EventTarget | null): boolean => {
  if (!(target instanceof HTMLElement)) return false
  const tag = target.tagName
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true
  return target.isContentEditable
}

export const useKeyboardShortcuts = (
  handlersRef: MutableRefObject<ShortcutHandlers>,
) => {
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const handlers = handlersRef.current
      const mod = e.metaKey || e.ctrlKey

      if (e.key === "Escape") {
        const handled = handlers.onCancel?.() ?? false
        if (handled) e.preventDefault()
        return
      }

      if (!mod) return

      if (e.key === "Enter" && handlers.onSend) {
        e.preventDefault()
        handlers.onSend()
        return
      }

      if (e.key === "n" && handlers.onNewSession) {
        e.preventDefault()
        handlers.onNewSession()
        return
      }

      if (e.key === "k" && handlers.onSearch) {
        e.preventDefault()
        handlers.onSearch()
        return
      }

      if (e.shiftKey && (e.key === "p" || e.key === "P") && handlers.onToggleCommandPalette) {
        e.preventDefault()
        handlers.onToggleCommandPalette()
        return
      }

      if (e.key === "b" && handlers.onToggleSidebar) {
        e.preventDefault()
        handlers.onToggleSidebar()
        return
      }

      if (e.key === "j" && handlers.onToggleRightPanel) {
        e.preventDefault()
        handlers.onToggleRightPanel()
        return
      }

      if (e.key === "w" && handlers.onCloseActiveTab) {
        if (e.ctrlKey && !e.metaKey && isEditableTarget(e.target)) return
        e.preventDefault()
        handlers.onCloseActiveTab()
        return
      }

      if (e.key === "l") {
        if (isEditableTarget(e.target)) return
        e.preventDefault()
        handlers.onFocusComposer?.()
      }
    }

    window.addEventListener("keydown", handleKeyDown)
    return () => window.removeEventListener("keydown", handleKeyDown)
  }, [handlersRef])
}

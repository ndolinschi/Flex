import { useAppStore } from "../../stores/appStore"
import { Toaster } from "@/components/ui/sonner"

/**
 * In-app toast host — mounts the Sonner Toaster at bottom-right.
 *
 * Deliberately does not suppress the Browser child webview. A corner toast
 * must never blank the open page for its lifetime; native content painting
 * over transient feedback is the safer failure mode.
 *
 * Toast content is driven by `pushToast`/`dismissToast` in appStore, which
 * bridge into sonner's imperative `toast()` API.
 */
export const ToastHost = () => {
  const theme = useAppStore((s) => s.theme)

  return (
    <Toaster theme={theme} position="bottom-right" />
  )
}

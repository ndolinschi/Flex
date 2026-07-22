import { useAppStore } from "../../stores/appStore"
import { Toaster } from "@/components/ui/sonner"

/**
 * In-app toast host — mounts the Sonner Toaster at bottom-right.
 *
 * `data-suppress-native-webview` is required so the Browser child webview
 * hides when a toast intersects its slot (native webviews paint above all DOM).
 *
 * Toast content is driven by `pushToast`/`dismissToast` in appStore, which
 * bridge into sonner's imperative `toast()` API.
 */
export const ToastHost = () => {
  const theme = useAppStore((s) => s.theme)

  return (
    <Toaster
      theme={theme}
      position="bottom-right"
      data-suppress-native-webview=""
    />
  )
}

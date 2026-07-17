import { Toaster } from "@/components/ui/sonner"

/** in-app toast host — Sonner bottom-right.
 * Marks `data-suppress-native-webview` so the Browser child webview hides
 * when a toast intersects its slot (native webviews paint above all DOM). */
export const ToastHost = () => {
  return (
    <div data-suppress-native-webview="" className="contents">
      <Toaster className="!z-[1000]" />
    </div>
  )
}

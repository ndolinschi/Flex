import { useEffect } from "react"
import { Check, TriangleAlert } from "lucide-react"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"

const TOAST_TIMEOUT_MS = 4000

type ToastRowProps = {
  id: string
  text: string
  kind: "success" | "error"
  action?: { label: string; onAction: () => void }
  onDismiss: (id: string) => void
}

const ToastRow = ({ id, text, kind, action, onDismiss }: ToastRowProps) => {
  useEffect(() => {
    const timer = window.setTimeout(() => onDismiss(id), TOAST_TIMEOUT_MS)
    return () => window.clearTimeout(timer)
  }, [id, onDismiss])

  const Icon = kind === "success" ? Check : TriangleAlert

  return (
    <div
      className={cn(
        "animate-toast-in flex items-center gap-2 rounded-md border border-stroke-3 px-3 py-2 text-left text-sm",
        "bg-panel/80 backdrop-blur-[40px]",
      )}
    >
      <Button
        variant="ghost"
        onClick={() => onDismiss(id)}
        className="h-auto min-w-0 flex-1 justify-start gap-2 px-0 py-0 font-normal hover:bg-transparent"
      >
        <Icon
          className={cn(
            "h-3.5 w-3.5 shrink-0",
            kind === "success" ? "text-green" : "text-red",
          )}
          aria-hidden
        />
        <span className="min-w-0 flex-1 text-ink">{text}</span>
      </Button>
      {action ? (
        <Button
          variant="ghost"
          onClick={() => {
            action.onAction()
            onDismiss(id)
          }}
          className="h-auto shrink-0 rounded-sm bg-accent px-2 py-1 text-xs font-medium text-accent-text hover:bg-accent-hover"
        >
          {action.label}
        </Button>
      ) : null}
    </div>
  )
}

/** in-app toast host — bottom-right, stacked, auto-dismissing.
 * Marks `data-suppress-native-webview` so the Browser child webview hides
 * when a toast intersects its slot (native webviews paint above all DOM). */
export const ToastHost = () => {
  const toasts = useAppStore((s) => s.toasts)
  const dismissToast = useAppStore((s) => s.dismissToast)

  if (toasts.length === 0) return null

  return (
    <div
      data-suppress-native-webview=""
      className="pointer-events-none fixed bottom-4 right-4 z-[1000] flex flex-col gap-2"
    >
      {toasts.map((t) => (
        <div key={t.id} className="pointer-events-auto">
          <ToastRow
            id={t.id}
            text={t.text}
            kind={t.kind}
            action={t.action}
            onDismiss={dismissToast}
          />
        </div>
      ))}
    </div>
  )
}

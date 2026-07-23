import { useAppStore } from "../../stores/appStore"
import { Toaster } from "@/components/ui/sonner"

export const ToastHost = () => {
  const theme = useAppStore((s) => s.theme)

  return (
    <Toaster theme={theme} position="bottom-right" />
  )
}

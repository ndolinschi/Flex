import type { CSSProperties } from "react"
import { Toaster as Sonner, type ToasterProps } from "sonner"
import {
  CircleCheckIcon,
  InfoIcon,
  Loader2Icon,
  OctagonXIcon,
  TriangleAlertIcon,
} from "@/components/icons"
import { useAppStore } from "@/stores/appStore"
import { cn } from "@/lib/utils"

/** App Toaster — themes via Flex `data-theme`, not next-themes. */
const Toaster = ({ className, ...props }: ToasterProps) => {
  const theme = useAppStore((s) => s.theme)

  return (
    <Sonner
      theme={theme === "light" ? "light" : "dark"}
      className={cn("toaster group", className)}
      position="bottom-right"
      duration={4000}
      icons={{
        success: <CircleCheckIcon className="size-4" />,
        info: <InfoIcon className="size-4" />,
        warning: <TriangleAlertIcon className="size-4" />,
        error: <OctagonXIcon className="size-4" />,
        loading: <Loader2Icon className="size-4 animate-spin" />,
      }}
      style={
        {
          "--normal-bg": "var(--color-panel)",
          "--normal-text": "var(--color-ink)",
          "--normal-border": "var(--color-stroke-3)",
          "--border-radius": "var(--radius-md)",
        } as CSSProperties
      }
      toastOptions={{
        classNames: {
          toast: "cn-toast border-stroke-3 bg-panel/80 text-ink backdrop-blur-[40px]",
          actionButton:
            "bg-accent text-accent-text hover:bg-accent-hover!",
        },
      }}
      {...props}
    />
  )
}

export { Toaster }

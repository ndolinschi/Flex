"use client"

import { Toaster as Sonner, type ToasterProps } from "sonner"
import {
  CircleCheckIcon,
  InfoIcon,
  TriangleAlertIcon,
  OctagonXIcon,
  Loader2Icon,
} from "lucide-react"

const Toaster = ({ theme = "system", ...props }: ToasterProps) => {
  return (
    <Sonner
      theme={theme as ToasterProps["theme"]}
      className="toaster group"
      icons={{
        success: <CircleCheckIcon className="size-4 text-success" />,
        info: <InfoIcon className="size-4 text-ink-muted" />,
        warning: <TriangleAlertIcon className="size-4 text-warning" />,
        error: <OctagonXIcon className="size-4 text-danger" />,
        loading: <Loader2Icon className="size-4 animate-spin text-ink-muted" />,
      }}
      style={
        {
          // Quiet solid panel for every kind — no rainbow fills, no glass blur.
          "--normal-bg": "var(--color-panel)",
          "--normal-text": "var(--color-ink)",
          "--normal-border": "var(--color-stroke-3)",
          "--border-radius": "var(--radius-md)",
          "--success-bg": "var(--color-panel)",
          "--success-text": "var(--color-ink)",
          "--success-border": "var(--color-stroke-3)",
          "--error-bg": "var(--color-panel)",
          "--error-text": "var(--color-ink)",
          "--error-border": "var(--color-stroke-3)",
          "--warning-bg": "var(--color-panel)",
          "--warning-text": "var(--color-ink)",
          "--warning-border": "var(--color-stroke-3)",
          "--info-bg": "var(--color-panel)",
          "--info-text": "var(--color-ink)",
          "--info-border": "var(--color-stroke-3)",
        } as React.CSSProperties
      }
      toastOptions={{
        classNames: {
          // shadow-popover already includes the 1px stroke ring — no border.
          toast: "cn-toast bg-panel text-ink shadow-popover",
          description: "text-ink-muted",
          actionButton: "bg-fill-2 text-ink",
          cancelButton: "bg-fill-4 text-ink-muted",
        },
      }}
      {...props}
    />
  )
}

export { Toaster }

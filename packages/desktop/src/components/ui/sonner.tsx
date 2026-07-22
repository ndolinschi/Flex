"use client"

import { Toaster as Sonner, type ToasterProps } from "sonner"
import { CircleCheckIcon, InfoIcon, TriangleAlertIcon, OctagonXIcon, Loader2Icon } from "lucide-react"

const Toaster = ({ theme = "system", ...props }: ToasterProps) => {
  return (
    <Sonner
      theme={theme as ToasterProps["theme"]}
      className="toaster group"
      icons={{
        success: (
          <CircleCheckIcon className="size-4" />
        ),
        info: (
          <InfoIcon className="size-4" />
        ),
        warning: (
          <TriangleAlertIcon className="size-4" />
        ),
        error: (
          <OctagonXIcon className="size-4" />
        ),
        loading: (
          <Loader2Icon className="size-4 animate-spin" />
        ),
      }}
      style={
        {
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
          // Quiet panel toast — no glass blur, no rainbow success/error fills.
          toast: "cn-toast border-stroke-3 bg-panel text-ink shadow-popover",
        },
      }}
      {...props}
    />
  )
}

export { Toaster }

import * as React from "react"

import { cn } from "@/lib/utils"

/** Selective typography helpers for settings / empty chrome — not for
 * `MarkdownBody` (GFM stays on its own renderer). */

function TypographyH1({ className, ...props }: React.ComponentProps<"h1">) {
  return (
    <h1
      data-slot="typography-h1"
      className={cn("text-[17px] font-medium leading-[21px] text-ink", className)}
      {...props}
    />
  )
}

function TypographyH2({ className, ...props }: React.ComponentProps<"h2">) {
  return (
    <h2
      data-slot="typography-h2"
      className={cn("text-sm font-medium leading-4 text-ink-secondary", className)}
      {...props}
    />
  )
}

function TypographyP({ className, ...props }: React.ComponentProps<"p">) {
  return (
    <p
      data-slot="typography-p"
      className={cn("text-base leading-[18px] text-ink-secondary", className)}
      {...props}
    />
  )
}

function TypographyMuted({ className, ...props }: React.ComponentProps<"p">) {
  return (
    <p
      data-slot="typography-muted"
      className={cn("text-sm text-ink-muted", className)}
      {...props}
    />
  )
}

export { TypographyH1, TypographyH2, TypographyP, TypographyMuted }

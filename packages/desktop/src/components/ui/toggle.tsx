"use client"

import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Toggle as TogglePrimitive } from "radix-ui"

import { cn } from "@/lib/utils"

const toggleVariants = cva(
  "group/toggle inline-flex items-center justify-center gap-1 rounded-sm text-sm font-medium whitespace-nowrap transition-all outline-none hover:bg-fill-4 hover:text-ink focus-visible:border-stroke-2 focus-visible:ring-1 focus-visible:ring-stroke-2 disabled:pointer-events-none disabled:opacity-50 data-[state=on]:bg-fill-3 data-[state=on]:text-ink [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3.5",
  {
    variants: {
      variant: {
        default: "bg-transparent",
        outline: "border border-stroke-3 bg-transparent hover:bg-fill-4",
      },
      size: {
        default: "h-8 min-w-8 px-2.5",
        sm: "h-6 min-w-6 rounded-sm px-1.5 text-xs [&_svg:not([class*='size-'])]:size-3",
        lg: "h-9 min-w-9 px-2.5",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
)

function Toggle({
  className,
  variant = "default",
  size = "default",
  ...props
}: React.ComponentProps<typeof TogglePrimitive.Root> &
  VariantProps<typeof toggleVariants>) {
  return (
    <TogglePrimitive.Root
      data-slot="toggle"
      className={cn(toggleVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Toggle, toggleVariants }

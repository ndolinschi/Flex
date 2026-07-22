import { Button as ButtonPrimitive } from "@base-ui/react/button"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

/**
 * shadcn Base UI Button — restyled for Flex density (DESIGN.md):
 * `rounded-md`, compact heights, neutral focus ring (no accent glow).
 */
const buttonVariants = cva(
  // `text-left` overrides the UA `<button>` default (`text-align: center`) —
  // otherwise a flex-1 label sits far from its leading icon (sidebar / menus).
  "group/button inline-flex shrink-0 items-center justify-center rounded-md border border-transparent bg-clip-padding text-left text-sm font-medium whitespace-nowrap transition-all outline-none select-none focus-visible:border-stroke-2 focus-visible:ring-1 focus-visible:ring-stroke-2 active:not-aria-[haspopup]:translate-y-px disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-1 aria-invalid:ring-destructive/20 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground hover:bg-primary/80",
        // Hover/expanded use `fill-4` (whisper), NOT `bg-accent` — in Flex
        // `@theme`, `bg-accent` is the product brand (near-white in dark) and
        // pairs with `text-accent-foreground` (dark). `text-foreground` on that
        // fill is white-on-white (ModelPicker / Mode open state).
        outline:
          "border-border bg-background hover:bg-fill-4 hover:text-foreground aria-expanded:bg-fill-4 aria-expanded:text-foreground",
        secondary:
          "bg-secondary text-secondary-foreground hover:bg-fill-4 aria-expanded:bg-secondary aria-expanded:text-secondary-foreground",
        ghost:
          "hover:bg-fill-4 hover:text-foreground aria-expanded:bg-fill-4 aria-expanded:text-foreground",
        destructive:
          "bg-destructive/10 text-destructive hover:bg-destructive/20 focus-visible:border-destructive/40 focus-visible:ring-destructive/20",
        link: "text-primary underline-offset-4 hover:underline active:opacity-80",
      },
      size: {
        default:
          "h-8 gap-1.5 px-2.5 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2",
        xs: "h-6 gap-1 rounded-md px-2 text-xs has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3",
        sm: "h-7 gap-1 rounded-md px-2.5 text-[length:var(--text-sm)] has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3.5",
        lg: "h-9 gap-1.5 px-3 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2",
        icon: "size-8",
        "icon-xs": "size-6 rounded-md [&_svg:not([class*='size-'])]:size-3",
        "icon-sm": "size-7 rounded-md",
        "icon-lg": "size-9",
        /** Sidebar row actions — 20px chrome (DESIGN.md compact density). */
        "icon-2xs": "size-5 rounded-md [&_svg:not([class*='size-'])]:size-3",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
)

function Button({
  className,
  variant = "default",
  size = "default",
  ...props
}: ButtonPrimitive.Props & VariantProps<typeof buttonVariants>) {
  return (
    <ButtonPrimitive
      data-slot="button"
      // Merge AFTER cva so twMerge can override base `justify-center` with
      // caller `justify-start` (menu / sidebar rows). Passing className into
      // cva alone leaves both utilities in the string depending on compose.
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    />
  )
}

export { Button, buttonVariants }

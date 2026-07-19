import type { ReactNode } from "react"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { cn } from "@/lib/utils"

type IconButtonProps = Omit<
  React.ComponentProps<typeof Button>,
  "variant" | "size" | "children"
> & {
  label: string
  /** @deprecated Compose Spinner + disabled on `Button size="icon-*"` instead. */
  isLoading?: boolean
  /** Quiet chrome: idle opacity .5 → hover .8 (Feel: Opacity hover language). */
  quiet?: boolean
  children: ReactNode
  /** Prefer `icon-xs` (h-6) in panel headers; default `icon-sm` (h-7). */
  size?: "icon-xs" | "icon-sm" | "icon" | "icon-lg"
}

/**
 * Icon-only control — thin wrapper over shadcn `Button` icon sizes.
 * New code: use `<Button variant="ghost" size="icon-sm" aria-label=…>` directly.
 */
export const IconButton = ({
  label,
  isLoading = false,
  quiet = false,
  disabled,
  className,
  children,
  size = "icon-sm",
  type = "button",
  ...props
}: IconButtonProps) => {
  return (
    <Button
      type={type}
      variant="ghost"
      size={size}
      aria-label={label}
      title={label}
      disabled={disabled || isLoading}
      className={cn(
        "text-muted-foreground hover:bg-muted hover:text-foreground",
        quiet && "opacity-50 hover:opacity-80",
        className,
      )}
      {...props}
    >
      {isLoading ? <Spinner className="size-3.5" /> : children}
    </Button>
  )
}

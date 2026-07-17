import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react"
import { cn } from "../../lib/utils"
import { Spinner } from "./Spinner"

type IconButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string
  isLoading?: boolean
  /** Quiet chrome: idle opacity .5 → hover .8 (Feel: Opacity hover language). */
  quiet?: boolean
  children: ReactNode
}

/** Icon-only control. Forwards refs so Radix `asChild` triggers (menus, popovers) work. */
export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(
  function IconButton(
    {
      label,
      isLoading = false,
      quiet = false,
      disabled,
      className,
      children,
      ...props
    },
    ref,
  ) {
    return (
      <button
        ref={ref}
        type="button"
        aria-label={label}
        title={label}
        disabled={disabled || isLoading}
        className={cn(
          "inline-flex h-7 w-7 items-center justify-center rounded-sm",
          "text-ink-muted hover:bg-fill-4 hover:text-ink",
          "transition-[color,background-color,opacity] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
          "disabled:opacity-50 disabled:pointer-events-none",
          quiet && "opacity-50 hover:opacity-80",
          className,
        )}
        {...props}
      >
        {isLoading ? <Spinner size="sm" /> : children}
      </button>
    )
  },
)

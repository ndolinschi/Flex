import type { ButtonHTMLAttributes, ReactNode } from "react"
import { cn } from "../../lib/utils"
import { Spinner } from "./Spinner"

type IconButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string
  isLoading?: boolean
  children: ReactNode
}

export const IconButton = ({
  label,
  isLoading = false,
  disabled,
  className,
  children,
  ...props
}: IconButtonProps) => {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled || isLoading}
      className={cn(
        "inline-flex h-7 w-7 items-center justify-center rounded-md",
        "text-ink-muted hover:bg-surface-muted hover:text-ink",
        "transition-colors disabled:opacity-50 disabled:pointer-events-none",
        className,
      )}
      {...props}
    >
      {isLoading ? <Spinner size="sm" /> : children}
    </button>
  )
}

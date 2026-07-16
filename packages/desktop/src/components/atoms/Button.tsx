import type { ButtonHTMLAttributes, ReactNode } from "react"
import { cn } from "../../lib/utils"
import { Spinner } from "./Spinner"

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger"
type ButtonSize = "sm" | "md" | "lg"

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant
  size?: ButtonSize
  isLoading?: boolean
  children: ReactNode
}

const variantClasses: Record<ButtonVariant, string> = {
  primary:
    "bg-accent text-accent-text hover:bg-accent-hover border border-transparent",
  secondary:
    "bg-surface-muted text-ink border border-border hover:bg-surface-raised",
  ghost:
    "bg-transparent text-ink-secondary hover:bg-surface-muted border border-transparent",
  danger:
    "bg-danger-subtle text-danger border border-danger/20 hover:bg-danger/10",
}

const sizeClasses: Record<ButtonSize, string> = {
  // Use explicit length utilities — never `text-base`/`text-sm` alone for
  // color-bearing buttons: those names can resolve to color tokens.
  sm: "h-7 px-2.5 text-[length:var(--text-sm)] leading-[var(--text-sm--line-height)] gap-1.5",
  md: "h-8 px-3 text-[length:var(--text-sm)] leading-[var(--text-sm--line-height)] gap-1.5",
  lg: "h-9 px-4 text-[length:var(--text-base)] leading-[var(--text-base--line-height)] gap-2",
}

export const Button = ({
  variant = "primary",
  size = "md",
  isLoading = false,
  disabled,
  className,
  children,
  ...props
}: ButtonProps) => {
  return (
    <button
      type="button"
      disabled={disabled || isLoading}
      className={cn(
        "inline-flex items-center justify-center rounded-md font-medium",
        "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
        "disabled:opacity-50 disabled:pointer-events-none",
        variantClasses[variant],
        sizeClasses[size],
        className,
      )}
      {...props}
    >
      {isLoading ? <Spinner size="sm" /> : null}
      {children}
    </button>
  )
}

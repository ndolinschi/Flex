import type { ButtonHTMLAttributes, ReactNode } from "react"
import { Button as UiButton } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { Spinner } from "./Spinner"

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger"
type ButtonSize = "sm" | "md" | "lg"

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant
  size?: ButtonSize
  isLoading?: boolean
  children: ReactNode
}

/** Map legacy Flex variants/sizes onto shadcn `Button`. Keep `isLoading` as a
 * compat shim (compose Spinner) until call sites drop it. */
const variantMap = {
  primary: "default",
  secondary: "secondary",
  ghost: "ghost",
  danger: "destructive",
} as const

const sizeMap = {
  sm: "sm",
  md: "default",
  lg: "lg",
} as const

export const Button = ({
  variant = "primary",
  size = "md",
  isLoading = false,
  disabled,
  className,
  children,
  type = "button",
  ...props
}: ButtonProps) => {
  return (
    <UiButton
      type={type}
      variant={variantMap[variant]}
      size={sizeMap[size]}
      disabled={disabled || isLoading}
      className={cn(
        /* DESIGN.md: controls use rounded-md (8), not nova's rounded-lg */
        "rounded-md",
        className,
      )}
      {...props}
    >
      {isLoading ? <Spinner size="sm" /> : null}
      {children}
    </UiButton>
  )
}

import type { ReactNode } from "react"
import { Button as ShadcnButton } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"

/**
 * Flex atom adapter over shadcn Base UI `Button`.
 * Maps legacy Flex variant/size names onto the registry API so call sites
 * can migrate gradually; prefer importing `@/components/ui/button` for new code.
 *
 * Legacy → shadcn: primary→default, danger→destructive, md→default.
 * Drop `isLoading` at call sites eventually — compose Spinner + disabled.
 */
type FlexVariant = "primary" | "secondary" | "ghost" | "danger" | "outline" | "link"
type FlexSize = "sm" | "md" | "lg" | "xs"

type ShadcnVariant = "default" | "secondary" | "ghost" | "destructive" | "outline" | "link"
type ShadcnSize = "default" | "sm" | "lg" | "xs"

const VARIANT_MAP: Record<FlexVariant, ShadcnVariant> = {
  primary: "default",
  secondary: "secondary",
  ghost: "ghost",
  danger: "destructive",
  outline: "outline",
  link: "link",
}

const SIZE_MAP: Record<FlexSize, ShadcnSize> = {
  xs: "xs",
  sm: "sm",
  md: "default",
  lg: "lg",
}

type ButtonProps = Omit<
  React.ComponentProps<typeof ShadcnButton>,
  "variant" | "size"
> & {
  variant?: FlexVariant | ShadcnVariant
  size?: FlexSize | ShadcnSize | "icon" | "icon-xs" | "icon-sm" | "icon-lg" | "icon-2xs"
  /** @deprecated Compose `<Spinner data-icon="inline-start" />` + `disabled`. */
  isLoading?: boolean
  children: ReactNode
}

const resolveVariant = (
  variant: ButtonProps["variant"],
): ShadcnVariant => {
  if (!variant) return "default"
  if (variant in VARIANT_MAP) return VARIANT_MAP[variant as FlexVariant]
  return variant as ShadcnVariant
}

const resolveSize = (
  size: ButtonProps["size"],
): React.ComponentProps<typeof ShadcnButton>["size"] => {
  if (!size || size === "md") return "default"
  if (size in SIZE_MAP) return SIZE_MAP[size as FlexSize]
  return size as React.ComponentProps<typeof ShadcnButton>["size"]
}

export const Button = ({
  variant = "default",
  size = "default",
  isLoading = false,
  disabled,
  className,
  children,
  type = "button",
  ...props
}: ButtonProps) => {
  return (
    <ShadcnButton
      type={type}
      variant={resolveVariant(variant)}
      size={resolveSize(size)}
      disabled={disabled || isLoading}
      className={className}
      {...props}
    >
      {isLoading ? <Spinner data-icon="inline-start" className="size-3.5" /> : null}
      {children}
    </ShadcnButton>
  )
}

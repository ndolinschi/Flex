import type { ComponentProps, ReactNode } from "react"
import { Label as UiLabel } from "@/components/ui/label"
import { cn } from "@/lib/utils"

type LabelProps = Omit<ComponentProps<typeof UiLabel>, "children"> & {
  children: ReactNode
}

/** Thin wrap over shadcn `Label` with Flex secondary ink default. */
export const Label = ({ className, children, ...props }: LabelProps) => {
  return (
    <UiLabel
      className={cn("block text-ink-secondary", className)}
      {...props}
    >
      {children}
    </UiLabel>
  )
}

import type { LabelHTMLAttributes, ReactNode } from "react"
import { cn } from "../../lib/utils"

type LabelProps = LabelHTMLAttributes<HTMLLabelElement> & {
  children: ReactNode
}

export const Label = ({ className, children, ...props }: LabelProps) => {
  return (
    <label
      className={cn("block text-sm font-medium text-ink-secondary", className)}
      {...props}
    >
      {children}
    </label>
  )
}

import type { ReactNode } from "react"
import { Label } from "../atoms"
import { cn } from "../../lib/utils"

type FormFieldProps = {
  label: string
  htmlFor: string
  hint?: string
  error?: string
  children: ReactNode
  className?: string
}

export const FormField = ({
  label,
  htmlFor,
  hint,
  error,
  children,
  className,
}: FormFieldProps) => {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
      {hint && !error ? (
        <p className="text-xs text-ink-faint">{hint}</p>
      ) : null}
      {error ? (
        <p className="text-xs text-danger" role="alert">
          {error}
        </p>
      ) : null}
    </div>
  )
}

import type { ReactNode } from "react"
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@/components/ui/field"
import { cn } from "@/lib/utils"

type FormFieldProps = {
  label: string
  htmlFor: string
  hint?: string
  error?: string
  children: ReactNode
  className?: string
}

/** Label + control + hint/error — shadcn Field composition. */
export const FormField = ({
  label,
  htmlFor,
  hint,
  error,
  children,
  className,
}: FormFieldProps) => {
  return (
    <Field
      data-invalid={error ? true : undefined}
      className={cn("gap-1.5", className)}
    >
      <FieldLabel htmlFor={htmlFor} className="text-ink-secondary">
        {label}
      </FieldLabel>
      {children}
      {hint && !error ? (
        <FieldDescription className="text-xs text-ink-faint">
          {hint}
        </FieldDescription>
      ) : null}
      {error ? <FieldError className="text-xs">{error}</FieldError> : null}
    </Field>
  )
}

import type { ReactNode } from "react"
import { Alert, AlertDescription } from "@/components/ui/alert"
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
        <p className="text-xs text-muted-foreground">{hint}</p>
      ) : null}
      {error ? (
        <Alert
          variant="destructive"
          className="border-0 bg-transparent px-0 py-0"
        >
          <AlertDescription className="text-xs text-destructive">
            {error}
          </AlertDescription>
        </Alert>
      ) : null}
    </div>
  )
}

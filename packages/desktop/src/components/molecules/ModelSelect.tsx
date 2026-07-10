import type { ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Label, Spinner } from "../atoms"

type ModelSelectProps = {
  id: string
  label?: string
  models: ModelInfoDto[]
  value: string
  onChange: (value: string) => void
  isLoading?: boolean
  disabled?: boolean
  placeholder?: string
  className?: string
}

export const ModelSelect = ({
  id,
  label = "Model",
  models,
  value,
  onChange,
  isLoading = false,
  disabled = false,
  placeholder = "Select a model",
  className,
}: ModelSelectProps) => {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <Label htmlFor={id}>{label}</Label>
      <div className="relative">
        <select
          id={id}
          value={value}
          disabled={disabled || isLoading}
          onChange={(e) => onChange(e.target.value)}
          className={cn(
            "h-9 w-full appearance-none rounded-md border border-border bg-surface",
            "px-3 pr-8 text-sm text-ink",
            "focus:border-accent focus:outline-none focus:[box-shadow:0_0_0_1px_var(--color-accent)]",
            "disabled:cursor-not-allowed disabled:opacity-50",
          )}
        >
          <option value="">{placeholder}</option>
          {models.map((m) => (
            <option key={m.id} value={m.id}>
              {m.displayName ?? m.id}
            </option>
          ))}
        </select>
        {isLoading ? (
          <div className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2">
            <Spinner size="sm" />
          </div>
        ) : null}
      </div>
    </div>
  )
}

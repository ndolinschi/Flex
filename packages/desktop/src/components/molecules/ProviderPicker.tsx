import { ProviderIcon } from "../atoms/ProviderIcon"
import type { BuiltinProvider } from "../../lib/types"
import { cn } from "../../lib/utils"
import {
  ToggleGroup,
  ToggleGroupItem,
} from "@/components/ui/toggle-group"

type ProviderPickerProps = {
  providers: BuiltinProvider[]
  value: string
  onChange: (providerId: string) => void
  disabled?: boolean
  className?: string
}

/** Visual provider chooser — icon + label tiles instead of a bare `<select>`. */
export const ProviderPicker = ({
  providers,
  value,
  onChange,
  disabled = false,
  className,
}: ProviderPickerProps) => {
  return (
    <ToggleGroup
      value={[value]}
      onValueChange={(vals) => {
        // Exclusive select: pick the newly added value; ignore deselect clicks.
        const next = vals.find((v) => v !== value)
        if (next) onChange(next)
      }}
      disabled={disabled}
      aria-label="Provider"
      variant="outline"
      spacing={0}
      className={cn(
        "grid w-full grid-cols-2 gap-1.5 sm:grid-cols-3",
        className,
      )}
    >
      {providers.map((p) => (
        <ToggleGroupItem
          key={p.id}
          value={p.id}
          aria-label={p.label}
          className="h-9 w-full justify-start gap-2 rounded-md px-2 font-normal text-ink-secondary"
        >
          <ProviderIcon providerId={p.id} label={p.label} size={16} />
          <span className="min-w-0 truncate text-sm font-medium leading-none">
            {p.label}
          </span>
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  )
}

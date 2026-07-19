import { ProviderIcon } from "../atoms/ProviderIcon"
import type { BuiltinProvider } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"

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
    <ul
      role="listbox"
      aria-label="Provider"
      className={cn(
        "grid grid-cols-2 gap-1.5 sm:grid-cols-3",
        className,
      )}
    >
      {providers.map((p) => {
        const active = p.id === value
        return (
          <li key={p.id} role="option" aria-selected={active}>
            <Button
              variant="ghost"
              disabled={disabled}
              onClick={() => onChange(p.id)}
              className={cn(
                "h-9 w-full justify-start gap-2 rounded-md border px-2 font-normal",
                active
                  ? "border-accent bg-fill-2 text-ink hover:bg-fill-2"
                  : "border-stroke-3 bg-surface text-ink-secondary hover:border-stroke-2 hover:bg-fill-4 hover:text-ink",
              )}
            >
              <ProviderIcon providerId={p.id} label={p.label} size={16} />
              <span className="min-w-0 truncate text-sm font-medium leading-none">
                {p.label}
              </span>
            </Button>
          </li>
        )
      })}
    </ul>
  )
}

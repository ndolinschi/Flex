import { ProviderIcon } from "../atoms/ProviderIcon"
import type { BuiltinProvider } from "../../lib/types"
import { cn } from "../../lib/utils"

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
            <button
              type="button"
              disabled={disabled}
              onClick={() => onChange(p.id)}
              className={cn(
                // Symmetric 8px inset — avoid the old px-2.5 + flex-1 stretch
                // that left a bright empty pad on the right of short labels.
                "flex h-9 w-full items-center justify-start gap-2 rounded-md border px-2 text-left",
                "transition-colors duration-[var(--duration-fast)]",
                "disabled:pointer-events-none disabled:opacity-50",
                active
                  ? "border-accent bg-fill-2 text-ink"
                  : "border-stroke-3 bg-surface text-ink-secondary hover:border-stroke-2 hover:bg-fill-4 hover:text-ink",
              )}
            >
              <ProviderIcon providerId={p.id} label={p.label} size={16} />
              <span className="min-w-0 truncate text-sm font-medium leading-none">
                {p.label}
              </span>
            </button>
          </li>
        )
      })}
    </ul>
  )
}

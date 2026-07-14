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
                "flex w-full items-center gap-2 rounded-md border px-2.5 py-2 text-left",
                "transition-colors duration-[var(--duration-fast)]",
                "disabled:pointer-events-none disabled:opacity-50",
                active
                  ? "border-accent bg-fill-2 text-ink"
                  : "border-stroke-3 bg-surface text-ink-secondary hover:border-stroke-2 hover:bg-fill-4 hover:text-ink",
              )}
            >
              <ProviderIcon providerId={p.id} label={p.label} size={18} />
              <span className="min-w-0 flex-1 truncate text-sm font-medium">
                {p.label}
              </span>
            </button>
          </li>
        )
      })}
    </ul>
  )
}

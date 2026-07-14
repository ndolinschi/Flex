import type { ReactNode } from "react"
import { cn } from "../../lib/utils"

/* ── New Settings shell primitives (design-map/07-settings.md §4) ──────────
 * `SettingsCard` + `SettingRow` implement the reference's group-card/row
 * anatomy for the new SettingsShell nav+content layout. The legacy
 * `SettingsSection`/`FieldRow` pair below (bordered card, label+hint layout)
 * stays as-is — it's still used by list-style sections (MCP servers,
 * automations, provider connections) that don't fit the toggle-row shape. */

type SettingsCardProps = {
  /** Group label rendered OUTSIDE the card, above it (12px secondary per
   * the reference — not a card header). */
  label?: string
  description?: string
  children: ReactNode
  className?: string
}

/** Group card — flat panel-tier background, no border, rows separated by
 * inset dividers (see `SettingRow`). */
export const SettingsCard = ({
  label,
  description,
  children,
  className,
}: SettingsCardProps) => {
  return (
    <div className={cn("flex flex-col gap-2", className)}>
      {label ? (
        <div className="flex flex-col gap-0.5 pl-3.5 pr-3.5">
          <h3 className="text-sm leading-4 text-ink-secondary">{label}</h3>
          {description ? (
            <p className="text-sm leading-4 text-ink-muted">{description}</p>
          ) : null}
        </div>
      ) : null}
      <div className="flex flex-col rounded-[var(--radius-card)] bg-settings-card">{children}</div>
    </div>
  )
}

type SettingRowProps = {
  /** Stable id for search-index navigation + highlight targeting — see
   * `settingsSearchIndex.ts` and `SettingsShell`'s highlight effect. */
  rowId?: string
  title: string
  description?: string
  children?: ReactNode
  /** Suppress the top inset divider — pass for the first row in a card. */
  first?: boolean
  /** Stack title above a full-width control (tall pickers, grids). */
  stacked?: boolean
  className?: string
}

/** Toggle-row anatomy: title+description (both 13px, differ only by color)
 * on the left, a right-aligned control slot, and an absolute inset divider
 * between rows (design-map/07-settings.md §4, `.cursor-settings-cell`). */
export const SettingRow = ({
  rowId,
  title,
  description,
  children,
  first = false,
  stacked = false,
  className,
}: SettingRowProps) => {
  return (
    <div
      data-settings-row={rowId}
      className={cn(
        "relative flex gap-4 px-3.5 py-3",
        stacked ? "flex-col items-stretch" : "items-center",
        !first && "before:absolute before:inset-x-3.5 before:top-0 before:h-px before:bg-stroke-4 before:content-['']",
        className,
      )}
    >
      <div className="min-w-0 flex-1">
        <p className="text-base leading-[18px] text-ink">{title}</p>
        {description ? (
          <p className="mt-0.5 text-base leading-[18px] text-ink-secondary">
            {description}
          </p>
        ) : null}
      </div>
      {children ? (
        <div
          className={cn(
            "flex items-center gap-2",
            stacked ? "w-full justify-start" : "shrink-0 justify-end",
          )}
        >
          {children}
        </div>
      ) : null}
    </div>
  )
}

type SettingsSectionProps = {
  title: string
  description?: string
  actions?: ReactNode
  children: ReactNode
  className?: string
  /** Stable id for search-index navigation + highlight targeting — see
   * `settingsSearchIndex.ts` and `SettingsShell`'s highlight effect. Mirrors
   * `SettingRow`'s `rowId` for sections that use this older list-card shape
   * (MCP servers, automations, provider connections) instead of `SettingRow`. */
  rowId?: string
}

export const SettingsSection = ({
  title,
  description,
  actions,
  children,
  className,
  rowId,
}: SettingsSectionProps) => {
  return (
    <section data-settings-row={rowId} className={cn("mb-8", className)}>
      <div className="mb-2 flex items-start justify-between gap-4 px-3.5">
        <div className="min-w-0">
          <h2 className="text-sm leading-4 text-ink-secondary">{title}</h2>
          {description ? (
            <p className="mt-0.5 text-sm leading-4 text-ink-muted">{description}</p>
          ) : null}
        </div>
        {actions ? (
          <div className="flex shrink-0 items-center gap-2">{actions}</div>
        ) : null}
      </div>
      <div
        className={cn(
          "@container/settings rounded-[var(--radius-card)] bg-settings-card",
          // Inset dividers between rows (12px inset, absolute — not
          // full-width borders), per design-map/07-settings.md §4.
          "[&>*+*]:relative [&>*+*]:before:absolute [&>*+*]:before:inset-x-3.5 [&>*+*]:before:top-0 [&>*+*]:before:h-px [&>*+*]:before:bg-stroke-4 [&>*+*]:before:content-['']",
        )}
      >
        {children}
      </div>
    </section>
  )
}

type FieldRowProps = {
  label: string
  hint?: string
  htmlFor?: string
  children: ReactNode
  className?: string
}

export const FieldRow = ({
  label,
  hint,
  htmlFor,
  children,
  className,
}: FieldRowProps) => {
  return (
    <div
      className={cn(
        "grid grid-cols-1 items-start gap-2 px-3.5 py-3",
        "@[640px]/settings:grid-cols-[240px_1fr] @[640px]/settings:gap-6",
        className,
      )}
    >
      <div className="min-w-0">
        <label
          htmlFor={htmlFor}
          className="block text-xs text-ink-secondary"
        >
          {label}
        </label>
        {hint ? (
          <p className="mt-0.5 text-xs text-ink-faint">{hint}</p>
        ) : null}
      </div>
      <div className="min-w-0">{children}</div>
    </div>
  )
}

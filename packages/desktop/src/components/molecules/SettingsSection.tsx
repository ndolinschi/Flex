import type { ReactNode } from "react"
import { cn } from "../../lib/utils"

type SettingsSectionProps = {
  title: string
  description?: string
  actions?: ReactNode
  children: ReactNode
  className?: string
}

export const SettingsSection = ({
  title,
  description,
  actions,
  children,
  className,
}: SettingsSectionProps) => {
  return (
    <section className={cn("mb-8", className)}>
      <div className="mb-3 flex items-start justify-between gap-4">
        <div className="min-w-0">
          <h2 className="text-[13px] font-medium text-ink">{title}</h2>
          {description ? (
            <p className="mt-0.5 text-xs text-ink-muted">{description}</p>
          ) : null}
        </div>
        {actions ? (
          <div className="flex shrink-0 items-center gap-2">{actions}</div>
        ) : null}
      </div>
      <div className="@container/settings divide-y divide-stroke-3 rounded-lg border border-stroke-3">
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
        "grid grid-cols-1 items-start gap-2 px-4 py-3",
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
          <p className="mt-0.5 text-[11px] text-ink-faint">{hint}</p>
        ) : null}
      </div>
      <div className="min-w-0">{children}</div>
    </div>
  )
}

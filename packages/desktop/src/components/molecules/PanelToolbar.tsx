import { forwardRef, type HTMLAttributes, type ReactNode } from "react"
import { cn } from "../../lib/utils"

/**
 * Shared tool-panel chrome — extracted from BrowserToolbar + TerminalTab.
 *
 * - `host` (default): 30px, px-2.5, border-b, bg-bg — Browser reference
 * - `elevated`: 40px panel-toolbar recipe — Terminal reference
 * - `quiet`: 30px, px-2.5, no border-b — only when a secondary strip already
 *   owns the body separator (rare; prefer host)
 */
export type PanelToolbarVariant = "host" | "elevated" | "quiet"

export type PanelToolbarProps = HTMLAttributes<HTMLDivElement> & {
  children: ReactNode
  /** Trailing action cluster (icon buttons). Gets ml-auto gap-1. */
  actions?: ReactNode
  variant?: PanelToolbarVariant
}

const variantClass: Record<PanelToolbarVariant, string> = {
  host: "h-[var(--header-height)] min-h-[var(--header-height)] gap-1.5 border-b border-stroke-3 bg-bg px-2.5",
  elevated:
    "panel-toolbar h-[var(--panel-toolbar-height)] min-h-[var(--panel-toolbar-height)]",
  quiet: "h-[var(--header-height)] min-h-[var(--header-height)] gap-1.5 px-2.5",
}

/** Ghost icon-button tokens used on Browser / Terminal chrome. */
export const panelChromeIconClass =
  "text-ink-muted hover:bg-fill-4 hover:text-ink shrink-0"

/** Pressed / active fill for toggle icon buttons in panel chrome. */
export const panelChromeIconActiveClass =
  "bg-fill-2 text-ink hover:bg-fill-2"

export const PanelToolbar = forwardRef<HTMLDivElement, PanelToolbarProps>(
  function PanelToolbar(
    {
      children,
      actions,
      variant = "host",
      className,
      role = "toolbar",
      ...props
    },
    ref,
  ) {
    return (
      <div
        ref={ref}
        role={role}
        className={cn(
          "relative flex shrink-0 items-center",
          variantClass[variant],
          className,
        )}
        {...props}
      >
        {children}
        {actions != null ? (
          <div className="ml-auto flex shrink-0 items-center gap-1">{actions}</div>
        ) : null}
      </div>
    )
  },
)

type PanelToolbarTitleProps = {
  icon?: ReactNode
  children: ReactNode
  className?: string
  title?: string
}

/** Leading title block: optional icon + truncate text-sm. */
export const PanelToolbarTitle = ({
  icon,
  children,
  className,
  title,
}: PanelToolbarTitleProps) => (
  <div
    className={cn("flex min-w-0 flex-1 items-center gap-1.5", className)}
    title={title}
  >
    {icon != null ? (
      <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center text-ink-faint [&>svg]:h-3.5 [&>svg]:w-3.5">
        {icon}
      </span>
    ) : null}
    <span className="min-w-0 truncate text-sm text-ink">{children}</span>
  </div>
)

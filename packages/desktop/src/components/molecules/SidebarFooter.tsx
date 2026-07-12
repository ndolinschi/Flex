import { Moon, Settings, Sun } from "lucide-react"
import { IconButton, Spinner } from "../atoms"

type SidebarFooterProps = {
  theme: "dark" | "light"
  onToggleTheme: () => void
  onOpenSettings: () => void
  isCreating?: boolean
}

/** Theme + settings chrome; optional creating spinner below the border. */
export const SidebarFooter = ({
  theme,
  onToggleTheme,
  onOpenSettings,
  isCreating = false,
}: SidebarFooterProps) => {
  return (
    <>
      <div className="flex items-center justify-end gap-0.5 border-t border-stroke-3 px-2 py-1.5">
        <IconButton
          quiet
          label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
          onClick={onToggleTheme}
        >
          {theme === "dark" ? (
            <Sun className="h-3.5 w-3.5" aria-hidden />
          ) : (
            <Moon className="h-3.5 w-3.5" aria-hidden />
          )}
        </IconButton>
        <IconButton quiet label="Settings" onClick={onOpenSettings}>
          <Settings className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      </div>

      {isCreating ? (
        <div className="flex items-center gap-2 border-t border-stroke-3 px-4 py-2 text-xs text-ink-muted">
          <Spinner size="sm" />
          Creating…
        </div>
      ) : null}
    </>
  )
}

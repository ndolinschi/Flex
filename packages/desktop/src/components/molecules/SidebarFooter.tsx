import { Moon, Settings, Sun } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Spinner } from "../atoms"
import { cn } from "../../lib/utils"

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
      {/* Continuous sidebar surface: hairline uses sidebar-border (= stroke-3). */}
      <div className="flex items-center justify-end gap-0.5 border-t border-sidebar-border px-2.5 py-1.5">
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label={
            theme === "dark" ? "Switch to light theme" : "Switch to dark theme"
          }
          title={
            theme === "dark" ? "Switch to light theme" : "Switch to dark theme"
          }
          onClick={onToggleTheme}
          className={cn(
            "text-ink-secondary hover:bg-fill-4 hover:text-ink",
            "opacity-50 hover:opacity-80",
            "h-6 w-6",
          )}
        >
          {theme === "dark" ? (
            <Sun className="h-3.5 w-3.5" aria-hidden />
          ) : (
            <Moon className="h-3.5 w-3.5" aria-hidden />
          )}
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label="Settings"
          title="Settings"
          onClick={onOpenSettings}
          className={cn(
            "text-ink-secondary hover:bg-fill-4 hover:text-ink",
            "opacity-50 hover:opacity-80",
            "h-6 w-6",
          )}
        >
          <Settings className="h-3.5 w-3.5" aria-hidden />
        </Button>
      </div>

      {isCreating ? (
        <div className="flex items-center gap-2 border-t border-sidebar-border px-2.5 py-1.5 text-xs text-ink-muted">
          <Spinner size="sm" />
          Creating…
        </div>
      ) : null}
    </>
  )
}

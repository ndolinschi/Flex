import type { ReactNode } from "react"
import { ArrowLeft } from "lucide-react"
import { IconButton } from "../atoms"
import { useAppStore } from "../../stores/appStore"

type SettingsShellProps = {
  children: ReactNode
  /** Header title; back always returns to chat. */
  title?: string
  /** Optional supporting copy rendered under the title. */
  description?: string
  /** Rail width for the content column. */
  wide?: boolean
  /** When true, omit the outer full-window chrome (used inside AppShell). */
  embedded?: boolean
}

export const SettingsShell = ({
  children,
  title = "Provider settings",
  description,
  wide = false,
  embedded = false,
}: SettingsShellProps) => {
  const setRoute = useAppStore((s) => s.setRoute)

  const body = (
    <>
      <header className="flex min-h-[var(--header-height)] shrink-0 items-center gap-2 border-b border-stroke-3 px-8 py-6">
        <IconButton label="Back to chat" onClick={() => setRoute("chat")}>
          <ArrowLeft className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
        <div className="min-w-0">
          <h1 className="truncate text-[15px] font-medium text-ink">{title}</h1>
          {description ? (
            <p className="mt-0.5 truncate text-xs text-ink-muted">
              {description}
            </p>
          ) : null}
        </div>
      </header>
      <main className="flex-1 overflow-y-auto px-8 py-6">
        <div
          className={
            wide
              ? "mx-auto max-w-[var(--content-rail)]"
              : "mx-auto max-w-[720px]"
          }
        >
          {children}
        </div>
      </main>
    </>
  )

  if (embedded) {
    return <div className="flex h-full min-w-0 flex-1 flex-col bg-bg">{body}</div>
  }

  return <div className="flex h-full flex-col bg-bg">{body}</div>
}

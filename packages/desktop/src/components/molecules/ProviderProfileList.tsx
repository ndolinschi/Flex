import { Badge, ProviderIcon } from "../atoms"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { SettingsSection } from "./SettingsSection"
import type { ProviderProfileView } from "../../lib/types"
import { cn } from "../../lib/utils"

type ProviderProfileListProps = {
  profiles: ProviderProfileView[]
  editingId: string | null
  isActivating: boolean
  onNewConnection: () => void
  onSelect: (profile: ProviderProfileView) => void
  onActivate: (id: string) => void
  onDelete: (id: string) => void
}

/** Named provider connections list (Connections settings section). */
export const ProviderProfileList = ({
  profiles,
  editingId,
  isActivating,
  onNewConnection,
  onSelect,
  onActivate,
  onDelete,
}: ProviderProfileListProps) => {
  return (
    <SettingsSection
      title="Connections"
      description="Named provider connections you can switch between (e.g. two AWS accounts)"
      rowId="models-connections"
      className="mb-0"
      actions={
        <Button size="sm" variant="secondary" onClick={onNewConnection}>
          New connection
        </Button>
      }
    >
      {profiles.length === 0 ? (
        <div className="px-3.5 py-3 text-sm text-ink-muted">
          No connections yet — use New connection to add one.
        </div>
      ) : (
        profiles.map((p) => (
          <div
            key={p.id}
            role="button"
            tabIndex={0}
            onClick={() => onSelect(p)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") onSelect(p)
            }}
            className={cn(
              "flex cursor-pointer items-center justify-between gap-3 px-3.5 py-3 text-left transition-colors duration-[var(--duration-fast)] hover:bg-fill-4",
              editingId === p.id && "bg-fill-2",
            )}
          >
            <div className="min-w-0 flex-1">
              <div className="flex min-w-0 items-center gap-1.5">
                <ProviderIcon providerId={p.provider} size={16} />
                <span className="min-w-0 truncate text-sm font-medium text-ink">
                  {p.label}
                </span>
                <Badge variant="muted" className="shrink-0 px-1">
                  {p.provider}
                </Badge>
                {p.isActive ? (
                  <Badge variant="success" className="shrink-0 px-1">
                    Active
                  </Badge>
                ) : null}
                {!p.hasKey && p.provider !== "ollama" ? (
                  <Badge variant="warning" className="shrink-0 px-1">
                    No key
                  </Badge>
                ) : null}
              </div>
              {p.region || p.baseUrl ? (
                <p className="mt-0.5 truncate text-xs text-ink-faint">
                  {p.region ?? p.baseUrl}
                </p>
              ) : null}
            </div>
            <div
              className="flex shrink-0 items-center gap-1.5"
              onClick={(e) => e.stopPropagation()}
            >
              {!p.isActive ? (
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={isActivating}
                  onClick={() => onActivate(p.id)}
                >
                  {isActivating ? <Spinner data-icon="inline-start" /> : null}
                  Activate
                </Button>
              ) : null}
              <Button
                size="sm"
                variant="ghost"
                onClick={() => onDelete(p.id)}
              >
                Delete
              </Button>
            </div>
          </div>
        ))
      )}
    </SettingsSection>
  )
}

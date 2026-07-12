import { Hammer } from "lucide-react"
import { Button } from "../atoms"
import { cn } from "../../lib/utils"

type PlanBuildBarProps = {
  onBuild: () => void
  onKeepPlanning?: () => void
  isBuilding?: boolean
  /** Compact chip above the composer vs full Plan-tab footer. */
  variant?: "footer" | "chip"
  className?: string
}

/** Build CTA to leave plan mode and implement. */
export const PlanBuildBar = ({
  onBuild,
  onKeepPlanning,
  isBuilding = false,
  variant = "footer",
  className,
}: PlanBuildBarProps) => {
  if (variant === "chip") {
    return (
      <div
        className={cn(
          "flex items-center justify-center gap-2",
          className,
        )}
      >
        <Button
          variant="primary"
          size="md"
          isLoading={isBuilding}
          onClick={onBuild}
          aria-label="Build plan"
          className="min-w-[7.5rem] shadow-[var(--shadow-composer)]"
        >
          <Hammer className="h-3.5 w-3.5" aria-hidden />
          Build
        </Button>
      </div>
    )
  }

  return (
    <div
      className={cn(
        "flex shrink-0 items-center justify-end gap-2 border-t border-stroke-3 px-3 py-2.5",
        className,
      )}
    >
      {onKeepPlanning ? (
        <Button
          variant="ghost"
          size="sm"
          disabled={isBuilding}
          onClick={onKeepPlanning}
        >
          Keep planning
        </Button>
      ) : null}
      <Button
        variant="primary"
        size="md"
        isLoading={isBuilding}
        onClick={onBuild}
        aria-label="Build plan"
        className="min-w-[6.5rem]"
      >
        <Hammer className="h-3.5 w-3.5" aria-hidden />
        Build
      </Button>
    </div>
  )
}

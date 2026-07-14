import { Button } from "../atoms"
import { ErrorBanner } from "./ErrorBanner"
import type { PendingPermission } from "../../lib/types"
import { usePermissionRespond } from "../../hooks/usePermissionRespond"
import { cn } from "../../lib/utils"

type PermissionActionsProps = {
  permission: PendingPermission
  className?: string
}

/** Composer-footer Allow once / Always allow / Deny — replaces Send while a
 * tool permission is pending (Claude Code–style inline HITL). */
export const PermissionActions = ({
  permission,
  className,
}: PermissionActionsProps) => {
  const { isSubmitting, respond, error } = usePermissionRespond(permission)

  return (
    <div className={cn("flex min-w-0 flex-col items-end gap-1.5", className)}>
      {error ? (
        <div className="w-full max-w-sm">
          <ErrorBanner message={error} />
        </div>
      ) : null}
      <div
        className="flex flex-wrap items-center justify-end gap-1.5"
        role="group"
        aria-label="Permission decision"
      >
        {permission.options.includes("allow_once") ? (
          <Button
            size="sm"
            isLoading={isSubmitting}
            onClick={() => void respond("allow_once")}
          >
            Allow once
          </Button>
        ) : null}
        {permission.options.includes("allow_always") ? (
          <Button
            size="sm"
            variant="secondary"
            isLoading={isSubmitting}
            onClick={() => void respond("allow_always")}
          >
            Always allow
          </Button>
        ) : null}
        {permission.options.includes("deny") ? (
          <Button
            size="sm"
            variant="ghost"
            isLoading={isSubmitting}
            onClick={() => void respond("deny")}
            className="text-danger hover:bg-danger/10"
          >
            Deny
          </Button>
        ) : null}
      </div>
    </div>
  )
}

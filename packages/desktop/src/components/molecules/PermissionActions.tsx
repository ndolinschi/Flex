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
 * tool permission is pending (Claude Code–style inline HITL). `size="xs"`
 * keeps the footer row aligned with Plus / Mode / Model / Send controls. */
export const PermissionActions = ({
  permission,
  className,
}: PermissionActionsProps) => {
  const { isSubmitting, respond, error } = usePermissionRespond(permission)

  return (
    <div className={cn("flex min-w-0 flex-col items-end gap-1", className)}>
      {error ? (
        <div className="w-full max-w-sm">
          <ErrorBanner message={error} />
        </div>
      ) : null}
      <div
        className="flex flex-wrap items-center justify-end gap-1"
        role="group"
        aria-label="Permission decision"
      >
        {permission.options.includes("allow_once") ? (
          <Button
            size="xs"
            isLoading={isSubmitting}
            onClick={() => void respond("allow_once")}
          >
            Allow once
          </Button>
        ) : null}
        {permission.options.includes("allow_always") ? (
          <Button
            size="xs"
            variant="secondary"
            isLoading={isSubmitting}
            onClick={() => void respond("allow_always")}
          >
            Always allow
          </Button>
        ) : null}
        {permission.options.includes("deny") ? (
          <Button
            size="xs"
            variant="destructive"
            isLoading={isSubmitting}
            onClick={() => void respond("deny")}
          >
            Deny
          </Button>
        ) : null}
      </div>
    </div>
  )
}

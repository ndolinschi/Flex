import { useState } from "react"
import { Button } from "@/components/ui/button"
import { ButtonGroup } from "@/components/ui/button-group"
import { Spinner } from "@/components/ui/spinner"
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
  const [pendingAction, setPendingAction] = useState<string | null>(null)

  const handleRespond = (decision: string) => {
    setPendingAction(decision)
    void respond(decision).finally(() => setPendingAction(null))
  }

  return (
    <div className={cn("flex min-w-0 flex-col items-end gap-1", className)}>
      {error ? (
        <div className="w-full max-w-sm">
          <ErrorBanner message={error} />
        </div>
      ) : null}
      <ButtonGroup
        className="flex-wrap justify-end"
        aria-label="Permission decision"
      >
        {permission.options.includes("allow_once") ? (
          <Button
            size="xs"
            disabled={isSubmitting}
            onClick={() => handleRespond("allow_once")}
          >
            {pendingAction === "allow_once" ? (
              <Spinner data-icon="inline-start" />
            ) : null}
            Allow once
          </Button>
        ) : null}
        {permission.options.includes("allow_always") ? (
          <Button
            size="xs"
            variant="secondary"
            disabled={isSubmitting}
            onClick={() => handleRespond("allow_always")}
          >
            {pendingAction === "allow_always" ? (
              <Spinner data-icon="inline-start" />
            ) : null}
            Always allow
          </Button>
        ) : null}
        {permission.options.includes("deny") ? (
          <Button
            size="xs"
            variant="destructive"
            disabled={isSubmitting}
            onClick={() => handleRespond("deny")}
          >
            {pendingAction === "deny" ? (
              <Spinner data-icon="inline-start" />
            ) : null}
            Deny
          </Button>
        ) : null}
      </ButtonGroup>
    </div>
  )
}

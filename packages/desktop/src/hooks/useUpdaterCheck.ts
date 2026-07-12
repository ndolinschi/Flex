import { useEffect, useRef } from "react"
import {
  checkForAppUpdate,
  installAppUpdateAndRelaunch,
} from "../lib/updater"
import { useAppStore } from "../stores/appStore"

/** Fire-and-forget update check after bootstrap. Surfaces a toast with an
 * Install action when a newer signed release is on the channel; soft-fails
 * (no toast) when the channel isn't published yet. */
export const useUpdaterCheck = (enabled: boolean) => {
  const pushToast = useAppStore((s) => s.pushToast)
  const ranRef = useRef(false)

  useEffect(() => {
    if (!enabled || ranRef.current) return
    ranRef.current = true

    const run = async () => {
      const result = await checkForAppUpdate()
      if (result.status !== "available") return

      pushToast(`Update ${result.version} available`, "success", {
        label: "Install",
        onAction: () => {
          void (async () => {
            try {
              await installAppUpdateAndRelaunch()
            } catch (err) {
              pushToast(
                err instanceof Error ? err.message : String(err),
                "error",
              )
            }
          })()
        },
      })
    }

    void run()
  }, [enabled, pushToast])
}

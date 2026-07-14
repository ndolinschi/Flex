import { useEffect } from "react"
import { initGlobalErrorLogging, log } from "../lib/debug/log"
import {
  DEFAULT_ACCENT_ID,
  DEFAULT_CUSTOM_ACCENT,
  isAccentId,
  normalizeAccentHex,
} from "../lib/accent"
import { isConfigured, resumeSession } from "../lib/tauri"
import type { AppRoute } from "../lib/types/ui"
import { restoreUiState, useAppStore } from "../stores/appStore"
import type { UiTheme } from "../stores/types"

/** Bootstrap hydration — restores persisted UI state into the store at
 * startup and routes to "welcome" or "chat" depending on whether the app is
 * configured. Pure move from App.tsx's original bootstrap `useEffect`; keeps
 * the exact hydration order, the boot logging, and the global error-logging
 * install so behavior is unchanged. */
export const useBootstrap = (
  setRoute: (route: AppRoute) => void,
  setTheme: (theme: UiTheme) => void,
) => {
  // Global window error/rejection logging — installed once, cheap no-op
  // per-event when debug logging is off (see `log.ts::initGlobalErrorLogging`).
  useEffect(() => {
    initGlobalErrorLogging()
  }, [])

  useEffect(() => {
    const bootstrap = async () => {
      const bootStartedAt = performance.now()
      log.info("boot", "bootstrap: starting")
      try {
        const [configured, ui] = await Promise.all([
          isConfigured(),
          restoreUiState(),
        ])
        log.debug("boot", "bootstrap: isConfigured + restoreUiState resolved", {
          configured,
        })

        if (typeof ui.debugLoggingEnabled === "boolean") {
          useAppStore.getState().setDebugLoggingEnabled(ui.debugLoggingEnabled)
        }
        if (typeof ui.crashReportingEnabled === "boolean") {
          useAppStore.getState().setCrashReportingEnabled(ui.crashReportingEnabled)
        }

        setTheme(ui.theme === "light" ? "light" : "dark")

        const accentCustomHex =
          normalizeAccentHex(ui.accentCustomHex ?? "") ?? DEFAULT_CUSTOM_ACCENT
        const accentId = isAccentId(ui.accentId) ? ui.accentId : DEFAULT_ACCENT_ID
        // Hydrate hex first so `setAccentId("custom")` resolves the right tokens.
        useAppStore.setState({ accentCustomHex })
        useAppStore.getState().setAccentId(accentId)

        if (!configured) {
          log.info("boot", "bootstrap: not configured, routing to welcome")
          setRoute("welcome")
          return
        }

        // Restore per-session open tabs BEFORE focusing a session —
        // `setActiveSessionId` re-derives `rightPanelOpen`/`rightPanelTab`
        // from `openTabsBySession`. Hydrating tabs after that (or after
        // `setRightPanelTab`) wiped the just-registered tab and could leave
        // the panel open+collapsed with an empty strip and no "+".
        if (ui.openTabsBySession) {
          useAppStore.getState().setOpenTabsBySession(ui.openTabsBySession)
        }

        if (ui.activeSessionId) {
          try {
            await resumeSession(ui.activeSessionId)
            useAppStore.getState().setActiveSessionId(ui.activeSessionId)
          } catch {
            useAppStore.getState().setActiveSessionId(null)
          }
        }

        if (ui.selectedModelId) {
          useAppStore.getState().setSelectedModelId(ui.selectedModelId)
        }

        if (ui.selectedIsolation) {
          useAppStore.getState().setSelectedIsolation(ui.selectedIsolation)
        }

        // Effort moved from a single global setting to per-model (reference
        // design: effort is picked FOR a specific model, in its dropdown row).
        // Migration: if a legacy `selectedEffort` exists and we haven't
        // captured any per-model efforts yet, apply it once to the current
        // model and drop the legacy value so this only runs the first launch
        // after upgrading.
        if (ui.effortByModel) {
          for (const [modelId, effort] of Object.entries(ui.effortByModel)) {
            useAppStore.getState().setEffortForModel(modelId, effort)
          }
        } else if (ui.selectedEffort) {
          const currentModel = useAppStore.getState().selectedModelId
          if (currentModel) {
            useAppStore.getState().setEffortForModel(currentModel, ui.selectedEffort)
          }
          useAppStore.getState().setSelectedEffort(null)
        }

        if (ui.composerMode) {
          useAppStore.getState().setComposerMode(ui.composerMode)
        }

        if (ui.defaultPermissionMode) {
          useAppStore.getState().setDefaultPermissionMode(ui.defaultPermissionMode)
        }

        if (typeof ui.notificationsEnabled === "boolean") {
          useAppStore.getState().setNotificationsEnabled(ui.notificationsEnabled)
        }
        if (typeof ui.completionSoundEnabled === "boolean") {
          useAppStore.getState().setCompletionSoundEnabled(ui.completionSoundEnabled)
        }

        if (ui.recentCwds?.length) {
          useAppStore.getState().setRecentCwds(ui.recentCwds)
        }

        if (ui.pinnedSessionIds) {
          useAppStore.getState().setPinnedSessionIds(ui.pinnedSessionIds)
        }
        if (ui.archivedSessionIds) {
          useAppStore.getState().setArchivedSessionIds(ui.archivedSessionIds)
        }

        if (ui.sidebarCollapsed) {
          useAppStore.getState().setSidebarCollapsed(true)
        }

        // With an active session, open/tab already came from
        // `setActiveSessionId` + restored `openTabsBySession`. Don't re-apply
        // the global `rightPanelOpen`/`rightPanelTab` flags — that used to
        // reopen an empty collapsed strip after tabs were wiped by the old
        // hydration order. Only restore the collapse preference when the
        // panel actually ended up open (session has remembered tabs).
        const afterSession = useAppStore.getState()
        if (
          !afterSession.activeSessionId &&
          ui.rightPanelOpen &&
          ui.rightPanelTab
        ) {
          afterSession.setRightPanelOpen(true)
          afterSession.setRightPanelTab(ui.rightPanelTab)
        }
        if (
          typeof ui.rightPanelCollapsed === "boolean" &&
          useAppStore.getState().rightPanelOpen
        ) {
          useAppStore.getState().setRightPanelCollapsed(ui.rightPanelCollapsed)
        }
        if (ui.planAnnotationsBySession) {
          useAppStore
            .getState()
            .setRestoredPlanAnnotations(ui.planAnnotationsBySession)
        }
        if (typeof ui.rightPanelWidth === "number") {
          useAppStore.getState().setRightPanelWidth(ui.rightPanelWidth)
        }
        if (typeof ui.sidebarWidth === "number") {
          useAppStore.getState().setSidebarWidth(ui.sidebarWidth)
        }

        setRoute("chat")
      } catch (err) {
        log.error("boot", "bootstrap: failed, routing to welcome", err)
        setRoute("welcome")
      } finally {
        log.info("boot", "bootstrap: finished", {
          durationMs: Math.round(performance.now() - bootStartedAt),
        })
        useAppStore.getState().setBootstrapped(true)
      }
    }

    void bootstrap()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [setRoute, setTheme])
}

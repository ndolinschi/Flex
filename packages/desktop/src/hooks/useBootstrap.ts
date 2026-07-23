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

        // Restore custom themes before applying active theme so DOM reflects
        // the persisted state on first paint.
        if (Array.isArray(ui.customThemes) && ui.customThemes.length > 0) {
          useAppStore.setState({ customThemes: ui.customThemes })
        }
        if (typeof ui.activeThemeId === "string") {
          useAppStore.getState().setActiveTheme(ui.activeThemeId)
        }

        if (!configured) {
          log.info("boot", "bootstrap: not configured, routing to welcome")
          setRoute("welcome")
          return
        }

        // Restore content layout (or migrate from legacy openTabs / chat tabs)
        // BEFORE focusing a session so panes already exist.
        if (ui.openTabsBySession) {
          useAppStore.getState().setOpenTabsBySession(ui.openTabsBySession)
        }
        if (ui.openChatSessionIds?.length) {
          useAppStore.getState().setOpenChatSessionIds(ui.openChatSessionIds)
        }
        {
          const { migrateToContentLayout } = await import(
            "../stores/contentLayoutModel"
          )
          const layout = migrateToContentLayout({
            contentLayout: ui.contentLayout,
            activeSessionId: ui.activeSessionId ?? null,
            openChatSessionIds: ui.openChatSessionIds,
            openTabsBySession: ui.openTabsBySession,
            rightPanelOpen: ui.rightPanelOpen,
          })
          // Boot always starts single unless user had persisted split layout.
          if (!ui.contentLayout && layout.mode === "split") {
            useAppStore.getState().setContentLayout({
              ...layout,
              mode: "single",
              focusedPane: 0,
              panes: [layout.panes[0]!],
            })
          } else {
            useAppStore.getState().setContentLayout(layout)
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
        // Hydrate via setState (like pin/archive) — setters would re-persist.
        {
          const sidebarPatch: {
            sidebarProjectSort?: "recency" | "alpha"
            sidebarProjectVisibility?: "active" | "all"
          } = {}
          if (
            ui.sidebarProjectSort === "recency" ||
            ui.sidebarProjectSort === "alpha"
          ) {
            sidebarPatch.sidebarProjectSort = ui.sidebarProjectSort
          }
          if (
            ui.sidebarProjectVisibility === "active" ||
            ui.sidebarProjectVisibility === "all"
          ) {
            sidebarPatch.sidebarProjectVisibility = ui.sidebarProjectVisibility
          }
          if (Object.keys(sidebarPatch).length > 0) {
            useAppStore.setState(sidebarPatch)
          }
        }

        if (ui.sidebarCollapsed) {
          useAppStore.getState().setSidebarCollapsed(true)
        }

        // Right panel stays closed on app start. Still hydrate width/tabs so
        // ⌘J and later session switches restore the strip the user left behind.
        if (ui.planAnnotationsBySession) {
          useAppStore
            .getState()
            .setRestoredPlanAnnotations(ui.planAnnotationsBySession)
        }
        if (typeof ui.rightPanelWidth === "number") {
          useAppStore.getState().setRightPanelWidth(ui.rightPanelWidth)
        }
        if (typeof ui.sidebarWidth === "number") {
          // Migrate pre-glass default (344) → Agents density (280).
          const width = ui.sidebarWidth === 344 ? 280 : ui.sidebarWidth
          useAppStore.getState().setSidebarWidth(width)
        }

        // Show the chat shell before resume_session so the Loading spinner
        // does not wait on engine resume (and any incidental index work).
        setRoute("chat")
        useAppStore.getState().setBootstrapped(true)

        const activeId = ui.activeSessionId
        if (activeId) {
          try {
            await resumeSession(activeId)
            useAppStore
              .getState()
              .setActiveSessionId(activeId, { panel: "closed" })
          } catch {
            useAppStore.getState().setActiveSessionId(null)
          }
        }
      } catch (err) {
        log.error("boot", "bootstrap: failed, routing to welcome", err)
        setRoute("welcome")
        useAppStore.getState().setBootstrapped(true)
      } finally {
        log.info("boot", "bootstrap: finished", {
          durationMs: Math.round(performance.now() - bootStartedAt),
        })
        // Idempotent if already set after chat hydration above.
        useAppStore.getState().setBootstrapped(true)
      }
    }

    void bootstrap()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [setRoute, setTheme])
}

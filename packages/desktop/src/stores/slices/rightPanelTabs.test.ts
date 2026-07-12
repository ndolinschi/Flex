import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { useAppStore, sessionScopeKey } from "../appStore"

/**
 * Regression coverage for BUG #37 (live-QA): open right-panel tabs reset
 * when switching sessions. Root cause — `rightPanelOpen`/`rightPanelTab`
 * are single global fields (one `<aside>`, one visible panel), but which
 * tabs a session has open (`openTabsBySession`) is per-session. Nothing
 * synced the two: switching to a session with no open tabs (or explicitly
 * closing the panel there) left `rightPanelOpen` false, and switching BACK
 * to a session that still had tabs recorded in `openTabsBySession` never
 * re-opened the panel to show them — from the user's perspective the tabs
 * had vanished, even though the underlying per-session set was intact.
 *
 * The fix hooks `setActiveSessionId` to re-derive `rightPanelOpen` +
 * `rightPanelTab` from the TARGET session's own `openTabsBySession` /
 * `selectedTabBySession` on every switch.
 */

const A = "sess-A"
const B = "sess-B"

// Snapshot / restore the (module-singleton) store around each test so cases
// don't leak into each other (same pattern as applyGlobalEvent.test.ts).
let snapshot: ReturnType<typeof useAppStore.getState>

beforeEach(() => {
  snapshot = useAppStore.getState()
  useAppStore.setState({
    activeSessionId: null,
    rightPanelOpen: false,
    rightPanelTab: "plan",
    openTabsBySession: {},
    selectedTabBySession: {},
    viewport: "wide",
  })
})

afterEach(() => {
  useAppStore.setState(snapshot, true)
})

describe("right panel per-session tab state (BUG #37)", () => {
  it("keeps a session's open tabs + selected tab across switching away and back", () => {
    useAppStore.getState().setActiveSessionId(A)
    useAppStore.getState().setRightPanelOpen(true)
    useAppStore.getState().setRightPanelTab("changes")
    useAppStore.getState().setRightPanelTab("terminal")

    expect(useAppStore.getState().openTabsBySession[sessionScopeKey(A)]).toEqual([
      "changes",
      "terminal",
    ])
    expect(useAppStore.getState().rightPanelTab).toBe("terminal")

    // Switch to a fresh session with no open tabs — new session default:
    // panel closed / no tabs.
    useAppStore.getState().setActiveSessionId(B)
    expect(useAppStore.getState().rightPanelOpen).toBe(false)
    expect(useAppStore.getState().openTabsBySession[sessionScopeKey(B)] ?? []).toEqual(
      [],
    )

    // Session A's own record must be untouched by visiting B.
    expect(useAppStore.getState().openTabsBySession[sessionScopeKey(A)]).toEqual([
      "changes",
      "terminal",
    ])

    // Switch back to A — panel must reopen on the previously-selected tab,
    // with both open tabs still present.
    useAppStore.getState().setActiveSessionId(A)
    expect(useAppStore.getState().rightPanelOpen).toBe(true)
    expect(useAppStore.getState().rightPanelTab).toBe("terminal")
    expect(useAppStore.getState().openTabsBySession[sessionScopeKey(A)]).toEqual([
      "changes",
      "terminal",
    ])
  })

  it("restores the session's last-selected tab, not just any open tab", () => {
    useAppStore.getState().setActiveSessionId(A)
    useAppStore.getState().setRightPanelOpen(true)
    useAppStore.getState().setRightPanelTab("changes")
    useAppStore.getState().setRightPanelTab("terminal")
    // Select "changes" last so it's the remembered tab, even though
    // "terminal" was opened more recently.
    useAppStore.getState().setRightPanelTab("changes")

    useAppStore.getState().setActiveSessionId(B)
    useAppStore.getState().setActiveSessionId(A)

    expect(useAppStore.getState().rightPanelTab).toBe("changes")
  })

  it("closing the last open tab on one session doesn't affect another session's tabs", () => {
    useAppStore.getState().setActiveSessionId(A)
    useAppStore.getState().setRightPanelOpen(true)
    useAppStore.getState().setRightPanelTab("changes")

    useAppStore.getState().setActiveSessionId(B)
    useAppStore.getState().setRightPanelOpen(true)
    useAppStore.getState().setRightPanelTab("terminal")
    useAppStore.getState().closeTab(sessionScopeKey(B), "terminal")

    expect(useAppStore.getState().openTabsBySession[sessionScopeKey(B)]).toEqual([])
    expect(useAppStore.getState().openTabsBySession[sessionScopeKey(A)]).toEqual([
      "changes",
    ])

    useAppStore.getState().setActiveSessionId(A)
    expect(useAppStore.getState().rightPanelOpen).toBe(true)
    expect(useAppStore.getState().rightPanelTab).toBe("changes")
  })

  it("a brand new session with no tabs keeps the panel closed", () => {
    useAppStore.getState().setActiveSessionId(A)
    useAppStore.getState().setRightPanelOpen(true)
    useAppStore.getState().setRightPanelTab("plan")

    useAppStore.getState().setActiveSessionId("sess-brand-new")
    expect(useAppStore.getState().rightPanelOpen).toBe(false)
  })
})

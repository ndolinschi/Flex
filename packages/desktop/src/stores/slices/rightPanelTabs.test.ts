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

  it("opening the panel from closed clears a persisted collapsed strip", () => {
    useAppStore.setState({
      rightPanelOpen: false,
      rightPanelCollapsed: true,
    })
    useAppStore.getState().setRightPanelOpen(true)
    expect(useAppStore.getState().rightPanelOpen).toBe(true)
    expect(useAppStore.getState().rightPanelCollapsed).toBe(false)
  })

  it("toggle expands a collapsed open panel instead of closing it", () => {
    useAppStore.setState({
      rightPanelOpen: true,
      rightPanelCollapsed: true,
    })
    useAppStore.getState().toggleRightPanel()
    expect(useAppStore.getState().rightPanelOpen).toBe(true)
    expect(useAppStore.getState().rightPanelCollapsed).toBe(false)

    useAppStore.getState().toggleRightPanel()
    expect(useAppStore.getState().rightPanelOpen).toBe(false)
  })

  it("toggleRightPanel seeds Plan when the session has no open tabs", () => {
    useAppStore.getState().setActiveSessionId(A)
    expect(useAppStore.getState().rightPanelOpen).toBe(false)
    expect(useAppStore.getState().openTabsBySession[sessionScopeKey(A)] ?? []).toEqual(
      [],
    )

    useAppStore.getState().toggleRightPanel()

    expect(useAppStore.getState().rightPanelOpen).toBe(true)
    expect(useAppStore.getState().rightPanelTab).toBe("plan")
    expect(useAppStore.getState().openTabsBySession[sessionScopeKey(A)]).toEqual([
      "plan",
    ])
  })

  it("toggleRightPanel reseeds the last-selected tab after the strip was emptied", () => {
    useAppStore.getState().setActiveSessionId(A)
    useAppStore.getState().setRightPanelOpen(true)
    useAppStore.getState().setRightPanelTab("changes")
    useAppStore.getState().closeTab(sessionScopeKey(A), "changes")
    useAppStore.getState().setRightPanelOpen(false)

    useAppStore.getState().toggleRightPanel()

    expect(useAppStore.getState().rightPanelOpen).toBe(true)
    expect(useAppStore.getState().rightPanelTab).toBe("changes")
    expect(useAppStore.getState().openTabsBySession[sessionScopeKey(A)]).toEqual([
      "changes",
    ])
  })

  it("setRightPanelTab plan registers an empty Plan tab without requiring a plan", () => {
    useAppStore.getState().setActiveSessionId(A)
    useAppStore.getState().setRightPanelOpen(true)
    useAppStore.getState().setRightPanelTab("plan")

    expect(useAppStore.getState().rightPanelTab).toBe("plan")
    expect(useAppStore.getState().openTabsBySession[sessionScopeKey(A)]).toEqual([
      "plan",
    ])
    expect(useAppStore.getState().sessionPlansBySession[A] ?? []).toEqual([])
  })

  it("setPendingPlanApproval registers the plan tab in openTabsBySession", () => {
    useAppStore.getState().setActiveSessionId(A)
    useAppStore.getState().setPendingPlanApproval({
      sessionId: A,
      planId: "plan-1",
      plan: "# Plan",
    })
    expect(useAppStore.getState().rightPanelOpen).toBe(true)
    expect(useAppStore.getState().rightPanelCollapsed).toBe(false)
    expect(useAppStore.getState().rightPanelTab).toBe("plan")
    expect(useAppStore.getState().openTabsBySession[sessionScopeKey(A)]).toEqual([
      "plan",
    ])
  })

  it("openWorkspaceFile opens the Files tab and selects the path", () => {
    useAppStore.setState({
      openFilesBySession: {},
      activeFileBySession: {},
      fileDraftsBySession: {},
    })
    useAppStore.getState().setActiveSessionId(A)
    useAppStore.getState().openWorkspaceFile(sessionScopeKey(A), "src/App.tsx")

    const state = useAppStore.getState()
    expect(state.rightPanelOpen).toBe(true)
    expect(state.rightPanelTab).toBe("files")
    expect(state.openTabsBySession[sessionScopeKey(A)]).toEqual(["files"])
    expect(state.openFilesBySession[sessionScopeKey(A)]).toEqual(["src/App.tsx"])
    expect(state.activeFileBySession[sessionScopeKey(A)]).toBe("src/App.tsx")
  })

  it("reopening an already-open file focuses that buffer without duplicating", () => {
    const key = sessionScopeKey(A)
    useAppStore.getState().setActiveSessionId(A)
    useAppStore.getState().openWorkspaceFile(key, "a.ts")
    useAppStore.getState().openWorkspaceFile(key, "b.ts")
    expect(useAppStore.getState().activeFileBySession[key]).toBe("b.ts")
    expect(useAppStore.getState().openFilesBySession[key]).toEqual(["a.ts", "b.ts"])

    useAppStore.getState().openWorkspaceFile(key, "a.ts")
    expect(useAppStore.getState().activeFileBySession[key]).toBe("a.ts")
    expect(useAppStore.getState().openFilesBySession[key]).toEqual(["a.ts", "b.ts"])
  })

  it("clearSessionPanelState drops Files buffers and open tabs for that session", () => {
    useAppStore.getState().setActiveSessionId(A)
    useAppStore.getState().openWorkspaceFile(sessionScopeKey(A), "a.ts")
    useAppStore.getState().setWorkspaceFileDraft(sessionScopeKey(A), "a.ts", "x")
    useAppStore.getState().setRightPanelTab("changes")

    useAppStore.getState().clearSessionPanelState(A)

    const state = useAppStore.getState()
    expect(state.openFilesBySession[sessionScopeKey(A)]).toBeUndefined()
    expect(state.activeFileBySession[sessionScopeKey(A)]).toBeUndefined()
    expect(state.fileDraftsBySession[sessionScopeKey(A)]).toBeUndefined()
    expect(state.openTabsBySession[sessionScopeKey(A)]).toBeUndefined()
    expect(state.selectedTabBySession[sessionScopeKey(A)]).toBeUndefined()
  })

  it("opening the Files tab with no buffers keeps the panel open", () => {
    useAppStore.getState().setActiveSessionId(A)
    useAppStore.getState().setRightPanelOpen(true)
    useAppStore.getState().setRightPanelTab("files")
    expect(useAppStore.getState().rightPanelOpen).toBe(true)
    expect(useAppStore.getState().rightPanelTab).toBe("files")
    expect(useAppStore.getState().openTabsBySession[sessionScopeKey(A)]).toEqual([
      "files",
    ])
    expect(
      useAppStore.getState().openFilesBySession[sessionScopeKey(A)] ?? [],
    ).toEqual([])
  })

  it("closing the last Files buffer collapses the panel when Files was alone", () => {
    useAppStore.getState().setActiveSessionId(A)
    useAppStore.getState().openWorkspaceFile(sessionScopeKey(A), "a.ts")
    expect(useAppStore.getState().rightPanelOpen).toBe(true)
    expect(useAppStore.getState().openTabsBySession[sessionScopeKey(A)]).toEqual([
      "files",
    ])

    useAppStore.getState().closeWorkspaceFile(sessionScopeKey(A), "a.ts")
    // FilesTab effect closes the panel tab + panel; store-only close leaves
    // the files tab registered until the UI effect runs — simulate that path.
    const key = sessionScopeKey(A)
    useAppStore.getState().closeTab(key, "files")
    const remaining = useAppStore.getState().openTabsBySession[key] ?? []
    if (remaining.length === 0) {
      useAppStore.getState().setRightPanelOpen(false)
    }
    expect(useAppStore.getState().rightPanelOpen).toBe(false)
    expect(useAppStore.getState().openTabsBySession[key] ?? []).toEqual([])
  })

  it("renameWorkspaceFile retargets open buffers, drafts, and active path", () => {
    const key = sessionScopeKey(A)
    useAppStore.getState().setActiveSessionId(A)
    useAppStore.getState().openWorkspaceFile(key, "src/old.ts")
    useAppStore.getState().openWorkspaceFile(key, "src/keep.ts")
    useAppStore.getState().setWorkspaceFileDraft(key, "src/old.ts", "draft")
    useAppStore.getState().setActiveWorkspaceFile(key, "src/old.ts")

    useAppStore.getState().renameWorkspaceFile(key, "src/old.ts", "src/new.ts")

    const state = useAppStore.getState()
    expect(state.openFilesBySession[key]).toEqual(["src/new.ts", "src/keep.ts"])
    expect(state.activeFileBySession[key]).toBe("src/new.ts")
    expect(state.fileDraftsBySession[key]).toEqual({ "src/new.ts": "draft" })
  })
})

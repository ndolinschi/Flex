import { useEffect, useState } from "react"
import {
  AlertTriangle,
  Globe,
  Loader2,
} from "lucide-react"
import { Button } from "../atoms"
import { useBrowserSession } from "../../hooks/useBrowserSession"
import { NATIVE_APP_REQUIRED } from "../../lib/browserPreview"
import { BrowserToolbar } from "./browser/BrowserToolbar"

/** Browser right-panel tab: toolbar + omnibar + content area.
 * Scoped to the active session. Only one native webview / iframe exists;
 * `browserOwnerSessionId` tracks which session's content it currently shows.
 * Navigating from a session takes ownership. A session that previously
 * navigated but lost ownership shows a "Page is open in another chat" state
 * with a button to reclaim the webview. Stays mounted when inactive (parent
 * hides via display:none).
 *
 * Layout: absolute-fill host under the tab bar so the native webview always
 * has a definite box (toolbar on top, content flex-1 below) — flex-only
 * hosts were measuring short and leaving a black gap under the page.
 *
 * All session/ownership/webview-bounds/navigation logic lives in
 * `useBrowserSession` (src/hooks/useBrowserSession.ts) — this component is
 * the chrome view that consumes it. Toolbar/overflow live under `./browser/`. */
export const BrowserTab = ({ active }: { active: boolean }) => {
  const {
    hostRef,
    contentRef,
    toolbarRef,
    browserUrl,
    browserLoading,
    browserStarted,
    viewportPreset,
    setViewportPreset,
    loadError,
    preview,
    showLiveContent,
    showElsewhere,
    commitNavigate,
    handleReclaim,
    handleScreenshot,
    handleHardReload,
    handleReload,
    handleCopyUrl,
    handleClearHistory,
    handleClearData,
    handleAskAgent,
    handleOpenDevtools,
    browserDesignMode,
    toggleDesignMode,
    browserBack,
    browserForward,
  } = useBrowserSession(active)

  const [editing, setEditing] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  const [showErrorDetails, setShowErrorDetails] = useState(false)

  // Collapse the expanded error details whenever a fresh error page renders.
  useEffect(() => {
    setShowErrorDetails(false)
  }, [loadError])

  return (
    // Absolute fill of the RightPanel body under the tab strip — height is
    // the full remaining panel, not whatever flex-1 happened to settle on.
    <div
      ref={hostRef}
      className="absolute inset-0 flex flex-col bg-bg"
    >
      <BrowserToolbar
        toolbarRef={toolbarRef}
        browserUrl={browserUrl}
        browserLoading={browserLoading}
        browserStarted={browserStarted}
        showLiveContent={showLiveContent}
        viewportPreset={viewportPreset}
        setViewportPreset={setViewportPreset}
        editing={editing}
        setEditing={setEditing}
        menuOpen={menuOpen}
        setMenuOpen={setMenuOpen}
        commitNavigate={commitNavigate}
        browserBack={browserBack}
        browserForward={browserForward}
        handleReload={handleReload}
        handleOpenDevtools={handleOpenDevtools}
        handleScreenshot={handleScreenshot}
        handleHardReload={handleHardReload}
        handleCopyUrl={handleCopyUrl}
        handleClearHistory={handleClearHistory}
        handleClearData={handleClearData}
        browserDesignMode={browserDesignMode}
        toggleDesignMode={toggleDesignMode}
      />

      {/* Native webview paints into this box (below the toolbar). */}
      <div
        ref={contentRef}
        className="relative z-0 min-h-0 flex-1 bg-bg"
      >
        {preview ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 bg-bg px-6">
            <Globe className="h-8 w-8 text-ink-faint opacity-60" aria-hidden />
            <p className="text-[14px] font-medium text-ink">Browser</p>
            <p className="max-w-[320px] text-center text-sm text-ink-muted">
              {NATIVE_APP_REQUIRED}
            </p>
          </div>
        ) : !browserStarted ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 bg-bg">
            <Globe className="h-8 w-8 text-ink-faint opacity-60" aria-hidden />
            <p className="text-[14px] font-medium text-ink">Browser</p>
            <p className="max-w-[300px] text-center text-sm text-ink-muted">
              Enter a URL above, or instruct the Agent to navigate and use the
              browser
            </p>
          </div>
        ) : showElsewhere ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 bg-bg">
            <p className="max-w-[280px] text-center text-sm text-ink-muted">
              Page is open in another chat
            </p>
            <Button variant="secondary" size="sm" onClick={handleReclaim}>
              Reload here
            </Button>
          </div>
        ) : showLiveContent && loadError ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 bg-bg px-6">
            <AlertTriangle className="h-8 w-8 text-danger opacity-80" aria-hidden />
            <p className="text-[14px] font-medium text-ink">
              Can't connect to server
            </p>
            <p className="max-w-[320px] text-center text-sm text-ink-muted">
              {loadError.message}
            </p>
            <div className="flex items-center gap-2">
              <Button variant="primary" size="sm" onClick={handleAskAgent}>
                Ask Agent
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setShowErrorDetails((v) => !v)}
              >
                {showErrorDetails ? "Hide Details" : "Show Details"}
              </Button>
            </div>
            {showErrorDetails ? (
              <pre className="max-w-[420px] overflow-x-auto rounded-md bg-fill-3 px-3 py-2 text-left text-xs text-ink-muted">
                {`GET ${browserUrl}\n${loadError.host} refused to connect\n${loadError.message}`}
              </pre>
            ) : null}
          </div>
        ) : (
          <div className="flex h-full w-full items-center justify-center bg-bg">
            {browserLoading ? (
              <Loader2
                className="h-5 w-5 animate-spin text-ink-muted"
                aria-label="Loading page"
              />
            ) : null}
          </div>
        )}
      </div>
    </div>
  )
}

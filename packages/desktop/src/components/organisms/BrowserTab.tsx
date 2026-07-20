import { useEffect, useState } from "react"
import {
  AlertTriangleIcon,
  Globe,
  Loader2,
} from "lucide-react"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
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
 * Layout: toolbar sibling above a body. The native webview maps 1:1 onto an
 * empty `data-browser-webview-slot` (no children). React empty/error/spinner
 * UI is a sibling overlay — never inside the slot — so bounds/spacing stay
 * CSS-controlled (`inset-0`, or e.g. `top-2` for extra top gap).
 *
 * All session/ownership/webview-bounds/navigation logic lives in
 * `useBrowserSession` (src/hooks/useBrowserSession.ts) — this component is
 * the chrome view that consumes it. Toolbar/overflow live under `./browser/`. */
export const BrowserTab = ({
  active,
  sessionId,
}: {
  active: boolean
  sessionId: string | null
}) => {
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
  } = useBrowserSession(active, sessionId)

  const [editing, setEditing] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  const [showErrorDetails, setShowErrorDetails] = useState(false)

  // Collapse the expanded error details whenever a fresh error page renders.
  useEffect(() => {
    setShowErrorDetails(false)
  }, [loadError])

  // Overlays are siblings of the empty webview slot — never inside it.
  // Live pages paint via the native child webview (no React cover).
  const showOverlay =
    preview ||
    !browserStarted ||
    showElsewhere ||
    (showLiveContent && !!loadError)

  return (
    <div
      ref={hostRef}
      className="relative flex h-full min-h-0 w-full flex-col bg-bg"
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

      <div className="relative min-h-0 flex-1">
        {/* Empty measure target — native child webview paints into this box. */}
        <div
          ref={contentRef}
          data-browser-webview-slot=""
          className="pointer-events-none absolute inset-0"
          aria-hidden
        />

        {showOverlay ? (
          <div className="absolute inset-0 z-10 bg-bg">
            {preview ? (
              <div className="flex h-full flex-col items-center justify-center gap-3 px-4">
                <Globe className="h-6 w-6 text-ink-faint opacity-60" aria-hidden />
                <p className="text-sm font-medium text-ink">Browser</p>
                <p className="max-w-[320px] text-center text-sm text-ink-muted">
                  {NATIVE_APP_REQUIRED}
                </p>
              </div>
            ) : !browserStarted ? (
              <div className="flex h-full flex-col items-center justify-center gap-3 px-4">
                <Globe className="h-6 w-6 text-ink-faint opacity-60" aria-hidden />
                <p className="text-sm font-medium text-ink">Browser</p>
                <p className="max-w-[300px] text-center text-sm text-ink-muted">
                  Enter a URL above, or instruct the Agent to navigate and use the
                  browser
                </p>
              </div>
            ) : showElsewhere ? (
              <div className="flex h-full flex-col items-center justify-center gap-3">
                <p className="max-w-[280px] text-center text-sm text-ink-muted">
                  Page is open in another chat
                </p>
                <Button variant="secondary" size="sm" onClick={handleReclaim}>
                  Reload here
                </Button>
              </div>
            ) : showLiveContent && loadError ? (
              <div className="flex h-full flex-col items-center justify-center gap-3 px-4">
                <Alert variant="destructive" className="max-w-md">
                  <AlertTriangleIcon />
                  <AlertTitle>Can't connect to server</AlertTitle>
                  <AlertDescription>{loadError.message}</AlertDescription>
                </Alert>
                <div className="flex items-center gap-2">
                  <Button variant="default" size="sm" onClick={handleAskAgent}>
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
                  <pre className="max-w-[420px] overflow-x-auto rounded-md bg-muted px-3 py-2 text-left text-xs text-muted-foreground">
                    {`GET ${browserUrl}\n${loadError.host} refused to connect\n${loadError.message}`}
                  </pre>
                ) : null}
              </div>
            ) : null}
          </div>
        ) : null}
        {showLiveContent && !loadError && browserLoading ? (
          <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center">
            <Loader2
              className="h-5 w-5 animate-spin text-ink-muted"
              aria-label="Loading page"
            />
          </div>
        ) : null}
      </div>
    </div>
  )
}

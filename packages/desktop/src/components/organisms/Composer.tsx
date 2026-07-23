import { useEffect, useRef, useState, type ReactNode } from "react"
import { BypassPermissionsButton } from "../atoms"
import {
  ComposerInput,
  ErrorBanner,
  ModePicker,
  ModelPicker,
  PermissionActions,
  PlusMenu,
  SendButton,
  modeAllowsBypass,
} from "../molecules"
import { useModels } from "../../hooks/useModels"
import { useSessions } from "../../hooks/useSessions"
import { useComposerAttachments } from "../../hooks/useComposerAttachments"
import { useComposerSend, useComposerModelChange } from "../../hooks/useComposerSend"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"
import {
  respondPermission,
  setTurnPermissionMode,
  toInvokeError,
} from "../../lib/tauri"
import { log } from "../../lib/debug/log"
import { ContextBar } from "./ContextBar"
import { ComposerQueue } from "./composer/ComposerQueue"

type ComposerProps = {
  /** When set, bind drafts/send to this session instead of global active. */
  sessionId?: string | null
  /** When false (visited/unfocused chat), disable editing; ContextBar stays
   * visible so project/workspace chrome does not vanish in a split pane. */
  interactive?: boolean
  /** Empty agent: ContextBar above the bubble; active chat: footer below.
   * Layout is always the large column composer (never compact pill). */
  isHero?: boolean
  /** Permission / question card stacked flush above the bubble (same rail). */
  dockedOverlay?: ReactNode
  /** Optional "N Working" workers pill above the bubble. */
  workersSlot?: ReactNode
}

/** Send control that narrow-selects draft emptiness so the parent Composer
 * (and its ModelPicker) do not re-render on every keystroke. */
const ComposerSendButton = ({
  sessionId,
  isStreaming,
  hasAttachments,
  onSend,
  onStop,
}: {
  sessionId: string | null
  isStreaming: boolean
  hasAttachments: boolean
  onSend: () => void
  onStop: () => void
}) => {
  const hasText = useAppStore((s) => {
    const draft = sessionId
      ? (s.draftsBySession[sessionId] ?? "")
      : s.orphanDraft
    return draft.trim().length > 0
  })
  const canSend = hasText || hasAttachments
  return (
    <SendButton
      isStreaming={isStreaming}
      canQueue={isStreaming && canSend}
      disabled={!canSend && !isStreaming}
      onSend={onSend}
      onStop={onStop}
    />
  )
}

/** Soft ambient + inset hairline — docked HITL paints the side/bottom stroke
 * so the seam with Permission/Question stays continuous (no top border/ring). */
const DOCKED_BUBBLE_SHADOW = "shadow-[var(--shadow-composer)]"
const DOCKED_BUBBLE_SHADOW_FOCUS =
  "focus-within:shadow-[var(--shadow-composer-focus)]"

/** Elevated composer card. Draft subscription lives in `ComposerInput` /
 * `ComposerSendButton` so ContextBar + ModelPicker stay stable across keystrokes.
 * Always a column bubble: textarea on top, bottom toolbar
 * `Plus | Mode | (spacer) | Model | Bypass | Send`. */
export const Composer = ({
  sessionId: sessionIdProp = null,
  interactive = true,
  isHero = false,
  dockedOverlay = null,
  workersSlot = null,
}: ComposerProps) => {
  const storeActiveSessionId = useAppStore((s) => s.activeSessionId)
  const activeSessionId = sessionIdProp ?? storeActiveSessionId
  const hasDockedOverlay = !!dockedOverlay
  const pendingPermission = useAppStore((s) =>
    s.pendingPermission && s.pendingPermission.sessionId === activeSessionId
      ? s.pendingPermission
      : null,
  )
  const setComposerDraft = useAppStore((s) => s.setComposerDraft)
  const composerMode = useAppStore((s) => s.composerMode)
  const setComposerMode = useAppStore((s) => s.setComposerMode)
  const sessionBypass = useAppStore((s) =>
    activeSessionId ? !!s.sessionBypassBySession[activeSessionId] : false,
  )
  const setSessionBypass = useAppStore((s) => s.setSessionBypass)
  const selectedModelId = useAppStore((s) => s.selectedModelId)
  const setSelectedModelId = useAppStore((s) => s.setSelectedModelId)
  const effortByModel = useAppStore((s) => s.effortByModel)
  const setEffortForModel = useAppStore((s) => s.setEffortForModel)
  const pushRecentCwd = useAppStore((s) => s.pushRecentCwd)
  const isBootstrapped = useAppStore((s) => s.isBootstrapped)
  const route = useAppStore((s) => s.route)
  const { models, builtinProviders, isLoading: modelsLoading } = useModels(
    isBootstrapped && route !== "welcome",
  )
  const { sessions } = useSessions()
  const slashRootRef = useRef<HTMLDivElement>(null)
  const textareaRef = useRef<HTMLTextAreaElement | null>(null)
  const [error, setError] = useState<string | null>(null)
  const syncedSessionRef = useRef<string | null>(null)

  const active = sessions.find((s) => s.id === activeSessionId)

  const selectedModelUsable: boolean | null = modelsLoading
    ? null
    : !!selectedModelId && models.some((m) => m.id === selectedModelId)

  const {
    attachments,
    addAttachment,
    removeAttachment,
    clearAttachments,
    error: attachmentError,
    setError: setAttachmentError,
    handlePick,
    handleDrop,
    handlePaste,
  } = useComposerAttachments()

  useEffect(() => {
    if (attachmentError) setError(attachmentError)
  }, [attachmentError])

  const {
    isStreaming,
    messageQueue,
    handleSend,
    handleStop,
    handleRemoveQueued,
    handleSendQueuedNow,
  } = useComposerSend({
    activeSessionId,
    active,
    setComposerDraft,
    attachments,
    addAttachment,
    clearAttachments,
    selectedModelId,
    selectedEffort: selectedModelId
      ? (effortByModel[selectedModelId] ?? null)
      : null,
    selectedModelUsable,
    setError,
  })

  const { handleModelChange } = useComposerModelChange({
    activeSessionId,
    selectedModelId,
    setSelectedModelId,
    models,
    isBootstrapped,
    setError,
  })

  useEffect(() => {
    const handleFocusRequest = () => textareaRef.current?.focus()
    window.addEventListener("flex:focus-composer", handleFocusRequest)
    return () =>
      window.removeEventListener("flex:focus-composer", handleFocusRequest)
  }, [])

  useEffect(() => {
    if (!activeSessionId || !active) return
    if (syncedSessionRef.current === activeSessionId) return
    syncedSessionRef.current = activeSessionId
    if (active.model) setSelectedModelId(active.model)
    if (active.cwd) pushRecentCwd(active.base_cwd || active.cwd)
  }, [activeSessionId, active, setSelectedModelId, pushRecentCwd])

  if (!activeSessionId) {
    return (
      <div className="px-2.5 pb-3 text-center text-sm text-ink-muted">
        Select or create a session to start chatting.
      </div>
    )
  }

  const handleToggleBypass = () => {
    if (!activeSessionId || !modeAllowsBypass(composerMode)) return
    const next = !sessionBypass
    setSessionBypass(activeSessionId, next)
    const sessionId = activeSessionId
    void (async () => {
      try {
        await setTurnPermissionMode(
          sessionId,
          next ? "bypass_permissions" : null,
        )
      } catch (err) {
        log.warn("session", "set_turn_permission_mode from composer shield", {
          sessionId,
          enabled: next,
          error: toInvokeError(err),
        })
      }
      if (!next) return
      const pending = useAppStore.getState().pendingPermission
      if (!pending || pending.sessionId !== sessionId) return
      try {
        await respondPermission({
          sessionId,
          requestId: pending.requestId,
          decision: "allow_once",
        })
        useAppStore.getState().setPendingPermission(null)
      } catch (err) {
        log.warn("session", "auto-allow pending after bypass shield", {
          sessionId,
          error: toInvokeError(err),
        })
      }
    })()
    if (next) {
      useAppStore
        .getState()
        .addSessionLogRow(
          activeSessionId,
          "Bypass permissions on for this session",
        )
    }
  }

  const contextBar = activeSessionId ? (
    <ContextBar
      cwd={active?.cwd}
      projectCwd={active ? active.base_cwd || active.cwd : undefined}
      sessionId={activeSessionId}
      disabled={!interactive}
      onError={setError}
      compact={isHero}
    />
  ) : null

  const inputEnabled =
    interactive &&
    isBootstrapped &&
    route !== "welcome" &&
    !pendingPermission

  const composerInput = (
    <ComposerInput
      sessionId={activeSessionId}
      composerMode={composerMode}
      isHero={isHero}
      cwd={active?.cwd}
      enabled={inputEnabled}
      anchorRef={slashRootRef}
      attachments={attachments}
      removeAttachment={removeAttachment}
      addAttachment={addAttachment}
      handlePaste={handlePaste}
      handleDrop={handleDrop}
      onSend={() => void handleSend()}
      textareaRefOut={textareaRef}
    />
  )

  const plusCluster = !pendingPermission ? (
    <PlusMenu
      onAttachFile={() => void handlePick("file")}
      onAttachImage={() => void handlePick("image")}
    />
  ) : (
    <span className="px-1 text-xs text-ink-muted">Waiting for permission…</span>
  )

  const modePicker = !pendingPermission ? (
    <ModePicker value={composerMode} onChange={setComposerMode} />
  ) : null

  const modelPicker = !pendingPermission ? (
    <ModelPicker
      models={models}
      value={selectedModelId}
      onChange={(id) => void handleModelChange(id)}
      isLoading={modelsLoading}
      effortFor={(modelId) => effortByModel[modelId] ?? null}
      onEffortChange={setEffortForModel}
      builtinProviders={builtinProviders}
    />
  ) : null

  const actionCluster = pendingPermission ? (
    <PermissionActions permission={pendingPermission} />
  ) : (
    <>
      <BypassPermissionsButton
        composerMode={composerMode}
        sessionBypass={sessionBypass}
        disabled={!activeSessionId}
        onToggle={handleToggleBypass}
      />
      <ComposerSendButton
        sessionId={activeSessionId}
        isStreaming={isStreaming}
        hasAttachments={attachments.length > 0}
        onSend={() => void handleSend()}
        onStop={() => void handleStop()}
      />
    </>
  )

  return (
    <div className="px-2.5 pt-1 pb-1.5">
      {error ? (
        <div className="mx-auto mb-1.5 max-w-[var(--content-rail)]">
          <ErrorBanner
            message={error}
            onDismiss={() => {
              setError(null)
              setAttachmentError(null)
            }}
          />
        </div>
      ) : null}

      {isHero && contextBar ? (
        <div className="mx-auto mb-1 w-full max-w-[var(--content-rail)]">
          {contextBar}
        </div>
      ) : null}

      <ComposerQueue
        items={messageQueue}
        onSendNow={handleSendQueuedNow}
        onRemove={handleRemoveQueued}
      />

      <div className="relative mx-auto flex w-full max-w-[var(--content-rail)] flex-col">
        {workersSlot}
        {dockedOverlay}
        <div
          ref={slashRootRef}
          data-composer-layout="hero"
          className={cn(
            "group/composer relative flex w-full flex-col gap-1.5 composer-card",
            hasDockedOverlay
              ? cn(
                  "rounded-b-[var(--radius-composer)] rounded-t-none",
                  "border-x border-b border-stroke-2",
                  DOCKED_BUBBLE_SHADOW,
                  DOCKED_BUBBLE_SHADOW_FOCUS,
                )
              : "composer-card-hero focus-within:composer-card-focus",
          )}
        >
          {composerInput}
          <div className="flex items-center gap-2 px-2.5 pb-1.5">
            <div className="flex min-w-0 items-center gap-1">
              {plusCluster}
              {modePicker}
            </div>
            <div className="min-w-0 flex-1" aria-hidden />
            <div className="flex shrink-0 items-center gap-1.5">
              {modelPicker}
              {actionCluster}
            </div>
          </div>
        </div>

        {!isHero && contextBar ? (
          <div className="mt-1 w-full">{contextBar}</div>
        ) : null}
      </div>
    </div>
  )
}

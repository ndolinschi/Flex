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
  isHero?: boolean
  /** Permission / question card stacked flush above the bubble (same rail). */
  dockedOverlay?: ReactNode
  /** Optional "N Working" workers pill above the bubble. */
  workersSlot?: ReactNode
}

/** Send control that narrow-selects draft emptiness so the parent Composer
 * (and its ModelPicker) do not re-render on every keystroke. */
const ComposerSendButton = ({
  isStreaming,
  hasAttachments,
  onSend,
  onStop,
}: {
  isStreaming: boolean
  hasAttachments: boolean
  onSend: () => void
  onStop: () => void
}) => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const hasText = useAppStore((s) => {
    const draft = activeSessionId
      ? (s.draftsBySession[activeSessionId] ?? "")
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

/** Drop-shadow only — no 1px ring — so a docked Permission/Question card can
 * share a continuous surface without a hairline at the seam. Side/bottom
 * stroke is painted with real borders instead. */
const DOCKED_BUBBLE_SHADOW =
  "shadow-[0_1px_2px_var(--shadow-ambient),0_8px_24px_-6px_var(--shadow-color)]"
const DOCKED_BUBBLE_SHADOW_FOCUS =
  "focus-within:shadow-[0_1px_2px_var(--shadow-ambient),0_10px_28px_-6px_var(--shadow-color)]"

/** reference glass expanded prompt — fill surface, soft elevation, no harsh outline.
 * Draft subscription lives in `ComposerInput` / `ComposerSendButton` so
 * ContextBar + ModelPicker stay stable across keystrokes. */
export const Composer = ({
  isHero = false,
  dockedOverlay = null,
  workersSlot = null,
}: ComposerProps) => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  // Docked Permission/Question sits as a sibling above this bubble — squash
  // top corners and drop the top shadow-ring so the seam reads as one panel.
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

  // Whether the currently-selected model id resolves to a model with a
  // configured provider. `null` while the list is still loading (don't block
  // a send on an unknown); `false` when a stored id (e.g. `bedrock/…`) has no
  // provider, so the picker shows "Select model" and the send must be blocked
  // rather than firing a doomed request. See useComposerSend.
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
    handlePaste,
    handleDrop,
  } = useComposerAttachments()

  // Surface attachment errors through the same banner as everything else.
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

  // Browser tab's load-error page "Ask Agent" button prefills the draft (via
  // `setComposerDraft`) then asks for focus through this event, since the
  // textarea ref lives here, not in the store.
  useEffect(() => {
    const handleFocusRequest = () => textareaRef.current?.focus()
    window.addEventListener("flex:focus-composer", handleFocusRequest)
    return () =>
      window.removeEventListener("flex:focus-composer", handleFocusRequest)
  }, [])

  // Sync composer model from the active session once per switch.
  useEffect(() => {
    if (!activeSessionId || !active) return
    if (syncedSessionRef.current === activeSessionId) return
    syncedSessionRef.current = activeSessionId
    if (active.model) setSelectedModelId(active.model)
    if (active.cwd) pushRecentCwd(active.base_cwd || active.cwd)
  }, [activeSessionId, active, setSelectedModelId, pushRecentCwd])

  if (!activeSessionId) {
    return (
      <div className="px-4 pb-3 text-center text-sm text-ink-muted">
        Select or create a session to start chatting.
      </div>
    )
  }

  const handleToggleBypass = () => {
    if (!activeSessionId || composerMode !== "agent") return
    const next = !sessionBypass
    setSessionBypass(activeSessionId, next)
    const sessionId = activeSessionId
    void (async () => {
      try {
        // Mid-run: push BypassPermissions into the in-flight turn so later
        // tools stop prompting. Clearing when turning off reverts to session
        // defaults for subsequent tools this turn.
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
      // Auto-resolve a pending ask for this session so the user isn't left
      // staring at Allow Bash after they already flipped the shield on.
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

  return (
    <div className="px-4 pt-1.5 pb-0.5">
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

      <div className="mx-auto mb-1 w-full max-w-[var(--content-rail)]">
        <ContextBar
          cwd={active?.cwd}
          projectCwd={active ? active.base_cwd || active.cwd : undefined}
          sessionId={activeSessionId}
          disabled={false}
          onError={setError}
        />
      </div>

      <ComposerQueue
        items={messageQueue}
        onSendNow={handleSendQueuedNow}
        onRemove={handleRemoveQueued}
      />

      {/* Overlay + bubble share one rail column so they sit flush (no page-bg
       * gap from ChatShell's old absolute `bottom-full` dock). */}
      <div className="relative mx-auto flex w-full max-w-[var(--content-rail)] flex-col">
        {workersSlot}
        {dockedOverlay}
        <div
          ref={slashRootRef}
          className={cn(
            // w-full is required: without a fixed width the pill sizes to content,
            // and the textarea's w-full creates a circular dependency that collapses
            // width (→ placeholder wraps → scrollHeight inflates → height locks at max).
            "relative flex w-full flex-col",
            "bg-user-bubble",
            hasDockedOverlay
              ? cn(
                  "rounded-b-[var(--radius-composer)] rounded-t-none",
                  // Continue the docked card's side/bottom stroke; omit the top
                  // ring from --shadow-composer so the seam stays clean.
                  "border-x border-b border-stroke-3",
                  DOCKED_BUBBLE_SHADOW,
                  DOCKED_BUBBLE_SHADOW_FOCUS,
                )
              : cn(
                  "rounded-[var(--radius-composer)]",
                  "shadow-[var(--shadow-composer)]",
                  "focus-within:shadow-[var(--shadow-composer-focus)]",
                ),
            "transition-[box-shadow,background-color] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
            "hover:bg-[color-mix(in_srgb,var(--color-user-bubble)_97%,white)]",
            "focus-within:hover:bg-user-bubble",
          )}
        >
          <ComposerInput
            composerMode={composerMode}
            isHero={isHero}
            cwd={active?.cwd}
            enabled={
              isBootstrapped && route !== "welcome" && !pendingPermission
            }
            anchorRef={slashRootRef}
            attachments={attachments}
            removeAttachment={removeAttachment}
            addAttachment={addAttachment}
            handlePaste={handlePaste}
            handleDrop={handleDrop}
            onSend={() => void handleSend()}
            textareaRefOut={textareaRef}
          />

          <div className="flex items-center justify-between gap-2 px-2 pb-1.5 pt-0">
            <div className="flex min-w-0 items-center gap-0.5">
              {!pendingPermission ? (
                <>
                  <PlusMenu
                    onAttachFile={() => void handlePick("file")}
                    onAttachImage={() => void handlePick("image")}
                  />
                  <ModePicker
                    value={composerMode}
                    onChange={setComposerMode}
                  />
                  <ModelPicker
                    models={models}
                    value={selectedModelId}
                    onChange={(id) => void handleModelChange(id)}
                    isLoading={modelsLoading}
                    effortFor={(modelId) => effortByModel[modelId] ?? null}
                    onEffortChange={setEffortForModel}
                    builtinProviders={builtinProviders}
                  />
                </>
              ) : (
                <span className="px-1 text-xs text-ink-muted">
                  Waiting for permission…
                </span>
              )}
            </div>
            <div className="flex shrink-0 items-center gap-0.5">
              {pendingPermission ? (
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
                    isStreaming={isStreaming}
                    hasAttachments={attachments.length > 0}
                    onSend={() => void handleSend()}
                    onStop={() => void handleStop()}
                  />
                </>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

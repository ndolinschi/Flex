import {
  useEffect,
  useMemo,
  useState,
  type KeyboardEvent,
} from "react"
import { useQuery } from "@tanstack/react-query"
import {
  Box,
  ChevronRight,
  ExternalLink,
  List,
  RefreshCw,
  Send,
} from "lucide-react"
import {
  ScrollArea,
  Tab,
  TextArea,
  TextInput,
  Tooltip,
} from "../../components/atoms"
import { EmptyState, ErrorBanner } from "../../components/molecules"
import { Button } from "@/components/ui/button"
import {
  browserApplyStyleOverrides,
  componentsDetect,
  componentsDetail,
  componentsList,
  toInvokeError,
  type ComponentDetail,
  type ComponentNode,
} from "../../lib/tauri"
import {
  COMPONENT_CSS_PROPERTIES,
  diffStyleDrafts,
  type ComponentStyleEditPayload,
} from "../../lib/componentDesign"
import { isDomAttachment } from "../../lib/types"
import type { SessionMeta } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"

type ComponentsTabProps = {
  active: boolean
  session: SessionMeta | undefined
}

type StyleDraft = Record<string, string>

const isComponentsSupported = (
  detect: { isReact: boolean; frameworks?: string[] } | undefined,
): boolean => {
  if (!detect) return false
  if ((detect.frameworks?.length ?? 0) > 0) return true
  return detect.isReact
}

const frameworkLabel = (frameworks: string[] | undefined): string => {
  const ids = frameworks?.length ? frameworks : ["react"]
  const names = ids.map((id) => {
    if (id === "react") return "React"
    if (id === "vue") return "Vue"
    if (id === "angular") return "Angular"
    return id
  })
  if (names.length === 1) return names[0]!
  if (names.length === 2) return `${names[0]} / ${names[1]}`
  return names.join(", ")
}

const buildTree = (
  components: ComponentNode[],
  roots: string[],
): Array<{ node: ComponentNode; depth: number }> => {
  const byId = new Map(components.map((c) => [c.id, c]))
  const out: Array<{ node: ComponentNode; depth: number }> = []
  const visited = new Set<string>()

  const walk = (id: string, depth: number) => {
    if (visited.has(id) || depth > 8) return
    visited.add(id)
    const node = byId.get(id)
    if (!node) return
    out.push({ node, depth })
    for (const child of node.children) {
      walk(child, depth + 1)
    }
  }

  const rootIds = roots.length > 0 ? roots : components.map((c) => c.id)
  for (const id of rootIds) {
    walk(id, 0)
  }
  for (const c of components) {
    if (!visited.has(c.id)) {
      out.push({ node: c, depth: 0 })
      visited.add(c.id)
    }
  }
  return out
}

const stylesFromDom = (
  styles: Record<string, string> | undefined,
): StyleDraft => {
  const draft: StyleDraft = {}
  if (!styles) return draft
  for (const prop of COMPONENT_CSS_PROPERTIES) {
    const v = styles[prop]
    if (v && v !== "none" && v !== "normal") {
      draft[prop] = v
    }
  }
  return draft
}

const triggerComposerSend = (): void => {
  window.dispatchEvent(new Event("flex:focus-composer"))
  window.requestAnimationFrame(() => {
    const ta = document.querySelector<HTMLTextAreaElement>("[data-composer]")
    if (!ta) return
    ta.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "Enter",
        metaKey: true,
        bubbles: true,
      }),
    )
  })
}

/** Components workspace — Terminal-style inventory, Files-style mini-tabs,
 * local mini-prompt that packages component context + CSS diffs for the agent. */
export const ComponentsTab = ({ active, session }: ComponentsTabProps) => {
  const cwd = session?.cwd?.trim() ?? ""
  const fallbackCwd = session?.base_cwd?.trim() || undefined
  const addAttachment = useAppStore((s) => s.addAttachment)
  const clearAttachments = useAppStore((s) => s.clearAttachments)
  const attachments = useAppStore((s) => s.attachments)
  const setComposerDraft = useAppStore((s) => s.setComposerDraft)
  const openToolBesideChat = useAppStore((s) => s.openToolBesideChat)
  const pushToast = useAppStore((s) => s.pushToast)
  const activeSessionId = useAppStore((s) => s.activeSessionId)

  const [listVisible, setListVisible] = useState(true)
  const [openIds, setOpenIds] = useState<string[]>([])
  const [activeId, setActiveId] = useState<string | null>(null)
  const [styleById, setStyleById] = useState<Record<string, StyleDraft>>({})
  const [baselineById, setBaselineById] = useState<Record<string, StyleDraft>>(
    {},
  )
  const [promptById, setPromptById] = useState<Record<string, string>>({})
  const [error, setError] = useState<string | null>(null)
  const [sending, setSending] = useState(false)

  const domTarget = useMemo(() => {
    const last = [...attachments].reverse().find(isDomAttachment)
    return last?.payload ?? null
  }, [attachments])

  useEffect(() => {
    setOpenIds([])
    setActiveId(null)
    setStyleById({})
    setBaselineById({})
    setPromptById({})
    setError(null)
  }, [cwd, fallbackCwd])

  const { data: detect, isFetching: detectFetching } = useQuery({
    queryKey: ["components-detect", cwd, fallbackCwd ?? ""],
    queryFn: () => componentsDetect(cwd, fallbackCwd),
    enabled: active && !!cwd,
    staleTime: 30_000,
  })

  const {
    data: list,
    isFetching: listFetching,
    refetch: refetchList,
  } = useQuery({
    queryKey: ["components-list", cwd, fallbackCwd ?? ""],
    queryFn: () => componentsList(cwd, fallbackCwd),
    enabled: active && !!cwd && isComponentsSupported(detect),
    staleTime: 15_000,
  })

  const { data: detail } = useQuery({
    queryKey: ["components-detail", cwd, fallbackCwd ?? "", activeId],
    queryFn: () => componentsDetail(cwd, activeId!, fallbackCwd),
    enabled: active && !!cwd && !!activeId,
  })

  const byId = useMemo(() => {
    const map = new Map<string, ComponentNode>()
    for (const c of list?.components ?? []) map.set(c.id, c)
    return map
  }, [list])

  const tree = useMemo(
    () => (list ? buildTree(list.components, list.roots) : []),
    [list],
  )

  const styleDraft = activeId ? (styleById[activeId] ?? {}) : {}
  const baseline = activeId ? (baselineById[activeId] ?? {}) : {}
  const localPrompt = activeId ? (promptById[activeId] ?? "") : ""
  const dirtyChanges = activeId
    ? diffStyleDrafts(baseline, styleDraft)
    : []

  // Seed CSS from Design Mode selection when opening / focusing a component.
  useEffect(() => {
    if (!activeId || !domTarget) return
    const seeded = stylesFromDom(domTarget.styles)
    setBaselineById((prev) =>
      prev[activeId] ? prev : { ...prev, [activeId]: seeded },
    )
    setStyleById((prev) =>
      prev[activeId] ? prev : { ...prev, [activeId]: seeded },
    )
  }, [activeId, domTarget?.selector])

  const openComponent = (id: string) => {
    setOpenIds((prev) => (prev.includes(id) ? prev : [...prev, id]))
    setActiveId(id)
  }

  const closeComponent = (id: string) => {
    setOpenIds((prev) => {
      const next = prev.filter((x) => x !== id)
      setActiveId((cur) => {
        if (cur !== id) return cur
        return next[next.length - 1] ?? null
      })
      return next
    })
    setStyleById((prev) => {
      const next = { ...prev }
      delete next[id]
      return next
    })
    setBaselineById((prev) => {
      const next = { ...prev }
      delete next[id]
      return next
    })
    setPromptById((prev) => {
      const next = { ...prev }
      delete next[id]
      return next
    })
  }

  const setProp = (property: string, value: string) => {
    if (!activeId) return
    setStyleById((prev) => ({
      ...prev,
      [activeId]: { ...(prev[activeId] ?? {}), [property]: value },
    }))
    if (domTarget?.selector) {
      void browserApplyStyleOverrides(domTarget.selector, {
        [property]: value,
      }).catch(() => {
        // Preview injection is best-effort.
      })
    }
  }

  const buildPayload = (
    d: ComponentDetail,
  ): ComponentStyleEditPayload => ({
    componentName: d.name,
    file: d.file,
    exportName: d.exportName,
    targetSelector: domTarget?.selector ?? null,
    propsSummary: d.props.map(
      (p) =>
        `${p.name}${p.optional ? "?" : ""}${p.typeHint ? `: ${p.typeHint}` : ""}`,
    ),
    dependencies: d.children.map((cid) => {
      const node = byId.get(cid)
      return node ? `${node.name} (${node.file})` : cid
    }),
    sourceSnippet: d.sourceSnippet,
    changes: dirtyChanges,
  })

  const sendToAgent = () => {
    if (!activeSessionId) {
      pushToast("No active session", "error")
      return
    }
    if (!detail || !activeId) {
      pushToast("Open a component first", "error")
      return
    }
    const instruction = localPrompt.trim()
    if (!instruction && dirtyChanges.length === 0) {
      pushToast("Describe a change or edit a CSS property", "error")
      return
    }

    setSending(true)
    try {
      const payload = buildPayload(detail)
      // Drop prior component-style chips so this send is scoped to the
      // active open component (the local mini-prompt's context).
      const keep = attachments.filter((a) => a.kind !== "component-style")
      clearAttachments()
      for (const att of keep) addAttachment(att)
      addAttachment({
        id: `${Date.now()}-component-style`,
        kind: "component-style",
        name: `${detail.name} edit`,
        payload,
      })
      // Visible instruction goes to the composer; packaged context is hidden
      // in the attachment (timeline shows a compact chip).
      setComposerDraft(
        instruction ||
          `Apply the style changes to ${detail.name} (${detail.file}).`,
        activeSessionId,
      )
      triggerComposerSend()
      setBaselineById((prev) => ({
        ...prev,
        [activeId]: { ...styleDraft },
      }))
      setPromptById((prev) => ({ ...prev, [activeId]: "" }))
      pushToast("Sent component edit to the agent", "success")
    } finally {
      setSending(false)
    }
  }

  const onPromptKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault()
      sendToAgent()
    }
  }

  const openBrowserHint = () => {
    if (!activeSessionId) return
    openToolBesideChat(activeSessionId, "browser")
  }

  if (!cwd) {
    return (
      <div className="absolute inset-0 flex flex-col">
        <EmptyState
          className="min-h-0 flex-1"
          title="No project folder"
          description="Pick a working directory for this session to discover React, Vue, or Angular components."
        />
      </div>
    )
  }

  if (detect && !isComponentsSupported(detect)) {
    return (
      <div className="absolute inset-0 flex flex-col">
        <EmptyState
          className="min-h-0 flex-1"
          title="No UI framework detected"
          description={detect.reason}
        />
      </div>
    )
  }

  const busy = detectFetching || listFetching

  return (
    <div className="absolute inset-0 flex flex-col">
      {/* Header — Terminal pattern */}
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1 px-2.5">
        <span className="min-w-0 flex-1 truncate text-xs text-ink-muted">
          {list
            ? `${list.components.length} component${list.components.length === 1 ? "" : "s"} · ${frameworkLabel(list.frameworks ?? detect?.frameworks)}`
            : busy
              ? "Scanning…"
              : "Components"}
        </span>
        <Tooltip label={listVisible ? "Hide list" : "Show list"}>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label={listVisible ? "Hide list" : "Show list"}
            title={listVisible ? "Hide list" : "Show list"}
            onClick={() => setListVisible((v) => !v)}
            className={cn(
              "text-muted-foreground hover:bg-muted hover:text-foreground",
              "opacity-50 hover:opacity-80",
              "h-6 w-6",
            )}
          >
            <List className="h-3.5 w-3.5" aria-hidden />
          </Button>
        </Tooltip>
        <Tooltip label="Refresh">
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label="Refresh"
            title="Refresh"
            onClick={() => {
              void refetchList().catch((err) => setError(toInvokeError(err)))
            }}
            className={cn(
              "text-muted-foreground hover:bg-muted hover:text-foreground",
              "opacity-50 hover:opacity-80",
              "h-6 w-6",
            )}
          >
            <RefreshCw
              className={cn("h-3.5 w-3.5", busy && "animate-spin")}
              aria-hidden
            />
          </Button>
        </Tooltip>
      </div>

      {error ? (
        <ErrorBanner
          message={error}
          className="shrink-0 rounded-none border-x-0 border-t-0 px-2.5 py-1.5 text-xs"
        />
      ) : null}

      {!list || list.components.length === 0 ? (
        <EmptyState
          className="min-h-0 flex-1"
          title={busy ? "Scanning…" : "No components found"}
          description={
            busy
              ? "Looking for PascalCase exports in .tsx / .jsx files."
              : "No PascalCase React exports under src/, app/, or components/."
          }
        />
      ) : (
        <div className="flex min-h-0 flex-1">
          {listVisible ? (
            <aside className="flex w-[180px] shrink-0 flex-col border-r border-stroke-3">
              <div className="flex h-6 shrink-0 items-center px-2.5 text-xs text-ink-muted">
                Available
              </div>
              <ScrollArea className="min-h-0 flex-1 py-1.5">
                <ul>
                  {tree.map(({ node, depth }) => {
                    const isOpen = openIds.includes(node.id)
                    const isActive = node.id === activeId
                    return (
                      <li key={node.id}>
                        <Button
                          variant="ghost"
                          onClick={() => openComponent(node.id)}
                          style={{ paddingLeft: `${10 + depth * 10}px` }}
                          className={cn(
                            "h-auto w-full justify-start gap-1 py-1.5 pr-2.5 text-xs font-normal",
                            isActive
                              ? "bg-fill-2 text-ink hover:bg-fill-2"
                              : isOpen
                                ? "bg-fill-3 text-ink-secondary hover:bg-fill-3"
                                : "text-ink-secondary hover:bg-fill-4 hover:text-ink",
                          )}
                        >
                          {depth > 0 ? (
                            <ChevronRight
                              className="h-3 w-3 shrink-0 text-icon-3"
                              aria-hidden
                            />
                          ) : (
                            <Box
                              className="h-3 w-3 shrink-0 text-icon-3"
                              aria-hidden
                            />
                          )}
                          <span className="min-w-0 truncate font-mono">
                            {node.name}
                          </span>
                        </Button>
                      </li>
                    )
                  })}
                </ul>
              </ScrollArea>
            </aside>
          ) : null}

          <main className="flex min-w-0 flex-1 flex-col">
            {/* Mini-tabs — FilesTab chip strip */}
            {openIds.length > 0 ? (
              <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1.5 overflow-x-auto border-b border-stroke-3 px-2.5">
                {openIds.map((id) => {
                  const node = byId.get(id)
                  const dirty =
                    diffStyleDrafts(
                      baselineById[id] ?? {},
                      styleById[id] ?? {},
                    ).length > 0 || !!(promptById[id] ?? "").trim()
                  return (
                    <Tab
                      key={id}
                      selected={id === activeId}
                      size="sm"
                      variant="chip"
                      title={node?.file ?? id}
                      onSelect={() => setActiveId(id)}
                      onClose={() => closeComponent(id)}
                      closeLabel={`Close ${node?.name ?? id}`}
                    >
                      {dirty ? "● " : ""}
                      {node?.name ?? id.split("#").pop()}
                    </Tab>
                  )
                })}
              </div>
            ) : null}

            {activeId && detail ? (
              <>
                <ComponentCanvas
                  detail={detail}
                  hasLiveTarget={!!domTarget}
                  onOpenBrowser={openBrowserHint}
                  styleDraft={styleDraft}
                  dirtyCount={dirtyChanges.length}
                  onSetProp={setProp}
                />
                {/* Local mini-prompt — independent of the main chat draft */}
                <div className="flex shrink-0 flex-col gap-1.5 border-t border-stroke-3 px-2.5 py-2">
                  {dirtyChanges.length > 0 ? (
                    <div className="flex flex-wrap gap-1">
                      {dirtyChanges.slice(0, 6).map((c) => (
                        <span
                          key={c.property}
                          className="inline-flex h-5 max-w-[12rem] items-center truncate rounded-[4px] border border-stroke-3 bg-fill-3 px-1 font-mono text-[11px] text-ink-secondary"
                          title={`${c.property}: ${c.from || "(unset)"} → ${c.to}`}
                        >
                          {c.property}
                        </span>
                      ))}
                      {dirtyChanges.length > 6 ? (
                        <span className="text-[11px] text-ink-faint">
                          +{dirtyChanges.length - 6}
                        </span>
                      ) : null}
                    </div>
                  ) : null}
                  <div className="flex items-end gap-1.5">
                    <TextArea
                      value={localPrompt}
                      onChange={(e) =>
                        setPromptById((prev) => ({
                          ...prev,
                          [activeId]: e.target.value,
                        }))
                      }
                      onKeyDown={onPromptKeyDown}
                      placeholder={`Describe changes to ${detail.name}…`}
                      rows={2}
                      aria-label="Component edit prompt"
                      className="min-h-[2.5rem] flex-1 resize-none text-sm"
                    />
                    <Tooltip label="Send to agent (⌘↵)">
                      <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Send to agent" title="Send to agent"
      disabled={sending}
      onClick={sendToAgent}
      className={cn(
        "text-muted-foreground hover:bg-muted hover:text-foreground",
        "h-8 w-8 shrink-0",
      )}
    >
      <Send className="h-3.5 w-3.5" aria-hidden />
    </Button>
                    </Tooltip>
                  </div>
                  <p className="text-[11px] text-ink-faint">
                    Sends component context, dependencies, and CSS diffs with
                    your instruction — the change list stays hidden in chat.
                  </p>
                </div>
              </>
            ) : (
              <EmptyState
                className="min-h-0 flex-1"
                title="Open a component"
                description="Pick a component from the list to preview, tweak CSS, and describe edits in the prompt below."
              />
            )}
          </main>
        </div>
      )}
    </div>
  )
}

type CanvasProps = {
  detail: ComponentDetail
  hasLiveTarget: boolean
  onOpenBrowser: () => void
  styleDraft: StyleDraft
  dirtyCount: number
  onSetProp: (property: string, value: string) => void
}

const ComponentCanvas = ({
  detail,
  hasLiveTarget,
  onOpenBrowser,
  styleDraft,
  dirtyCount,
  onSetProp,
}: CanvasProps) => (
  <ScrollArea className="min-h-0 flex-1">
    <div className="flex flex-col">
      {/* Neutral preview canvas */}
      <div
        className={cn(
          "relative flex shrink-0 flex-col items-center justify-center gap-2",
          "border-b border-stroke-3 bg-fill-5 px-3 py-6",
        )}
        style={{
          backgroundImage:
            "radial-gradient(circle at 1px 1px, color-mix(in srgb, var(--color-stroke-3) 80%, transparent) 1px, transparent 0)",
          backgroundSize: "12px 12px",
          minHeight: 100,
        }}
      >
        <div
          className={cn(
            "flex max-w-full flex-col items-center gap-1 rounded-md border border-stroke-3",
            "bg-surface px-4 py-3 text-center",
          )}
        >
          <span className="font-mono text-sm text-ink">{detail.name}</span>
          <span className="truncate text-xs text-ink-muted">{detail.file}</span>
          {dirtyCount > 0 ? (
            <span className="text-[11px] text-ink-faint">
              {dirtyCount} CSS change{dirtyCount === 1 ? "" : "s"} pending
            </span>
          ) : null}
        </div>
        {!hasLiveTarget ? (
          <Button
            variant="ghost"
            onClick={onOpenBrowser}
            className="h-auto gap-1 px-0 py-0 text-xs font-normal text-ink-muted hover:bg-transparent hover:text-ink"
          >
            <ExternalLink className="h-3 w-3" aria-hidden />
            Open Browser + Design Mode for live preview
          </Button>
        ) : (
          <span className="text-xs text-ink-muted">
            Live overrides apply to the Design Mode selection
          </span>
        )}
      </div>

      <div className="flex flex-col gap-3 px-2.5 py-2">
        {detail.props.length > 0 ? (
          <section>
            <h3 className="mb-1 text-xs font-medium text-ink-muted">Props</h3>
            <ul className="space-y-0.5">
              {detail.props.map((p) => (
                <li
                  key={p.name}
                  className="font-mono text-xs text-ink-secondary"
                >
                  {p.name}
                  {p.optional ? "?" : ""}
                  {p.typeHint ? (
                    <span className="text-ink-faint">: {p.typeHint}</span>
                  ) : null}
                </li>
              ))}
            </ul>
          </section>
        ) : null}

        {detail.children.length > 0 ? (
          <section>
            <h3 className="mb-1 text-xs font-medium text-ink-muted">
              Dependencies
            </h3>
            <ul className="space-y-0.5">
              {detail.children.map((cid) => (
                <li
                  key={cid}
                  className="truncate font-mono text-xs text-ink-secondary"
                >
                  {cid.split("#").pop()}
                  <span className="text-ink-faint">
                    {" "}
                    · {cid.split("#")[0]}
                  </span>
                </li>
              ))}
            </ul>
          </section>
        ) : null}

        <section>
          <h3 className="mb-1 text-xs font-medium text-ink-muted">
            CSS parameters
          </h3>
          <div className="flex flex-col gap-1.5">
            {COMPONENT_CSS_PROPERTIES.map((prop) => (
              <label
                key={prop}
                className="grid grid-cols-[7.5rem_1fr] items-center gap-2"
              >
                <span className="truncate font-mono text-xs text-ink-muted">
                  {prop}
                </span>
                <TextInput
                  value={styleDraft[prop] ?? ""}
                  onChange={(e) => onSetProp(prop, e.target.value)}
                  placeholder="—"
                  className="h-6 px-1.5 font-mono text-xs"
                />
              </label>
            ))}
          </div>
        </section>
      </div>
    </div>
  </ScrollArea>
)

import { useEffect, useMemo, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import {
  Box,
  ChevronRight,
  ExternalLink,
  RefreshCw,
  Save,
} from "lucide-react"
import { Button, IconButton, ScrollArea, TextInput } from "../../components/atoms"
import { EmptyState } from "../../components/molecules"
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
  type ComponentStyleChange,
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
  // Orphans not reached from roots (cycles / partial graphs).
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

/** Right-panel Components plugin — React inventory, CSS panel, Save → agent. */
export const ComponentsTab = ({ active, session }: ComponentsTabProps) => {
  const cwd = session?.cwd?.trim() ?? ""
  const addAttachment = useAppStore((s) => s.addAttachment)
  const attachments = useAppStore((s) => s.attachments)
  const openToolBesideChat = useAppStore((s) => s.openToolBesideChat)
  const pushToast = useAppStore((s) => s.pushToast)
  const activeSessionId = useAppStore((s) => s.activeSessionId)

  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [styleDraft, setStyleDraft] = useState<StyleDraft>({})
  const [baseline, setBaseline] = useState<StyleDraft>({})
  const [error, setError] = useState<string | null>(null)

  const domTarget = useMemo(() => {
    const last = [...attachments].reverse().find(isDomAttachment)
    return last?.payload ?? null
  }, [attachments])

  useEffect(() => {
    setSelectedId(null)
    setStyleDraft({})
    setBaseline({})
    setError(null)
  }, [cwd])

  const { data: detect, isFetching: detectFetching } = useQuery({
    queryKey: ["components-detect", cwd],
    queryFn: () => componentsDetect(cwd),
    enabled: active && !!cwd,
    staleTime: 30_000,
  })

  const {
    data: list,
    isFetching: listFetching,
    refetch: refetchList,
  } = useQuery({
    queryKey: ["components-list", cwd],
    queryFn: () => componentsList(cwd),
    enabled: active && !!cwd && !!detect?.isReact,
    staleTime: 15_000,
  })

  const { data: detail } = useQuery({
    queryKey: ["components-detail", cwd, selectedId],
    queryFn: () => componentsDetail(cwd, selectedId!),
    enabled: active && !!cwd && !!selectedId,
  })

  const tree = useMemo(
    () =>
      list ? buildTree(list.components, list.roots) : [],
    [list],
  )

  // Seed CSS panel from Design Mode selection when present.
  useEffect(() => {
    if (!domTarget) return
    const seeded = stylesFromDom(domTarget.styles)
    setBaseline(seeded)
    setStyleDraft(seeded)
  }, [domTarget?.selector])

  // When selecting a component without a live DOM target, clear style draft.
  useEffect(() => {
    if (!selectedId) return
    if (domTarget) return
    setBaseline({})
    setStyleDraft({})
  }, [selectedId, domTarget])

  const setProp = (property: string, value: string) => {
    setStyleDraft((prev) => ({ ...prev, [property]: value }))
    if (domTarget?.selector) {
      void browserApplyStyleOverrides(domTarget.selector, {
        [property]: value,
      }).catch(() => {
        // Preview injection is best-effort (browser may be closed).
      })
    }
  }

  const dirtyChanges = (): ComponentStyleChange[] => {
    const changes: ComponentStyleChange[] = []
    const keys = new Set([
      ...Object.keys(baseline),
      ...Object.keys(styleDraft),
    ])
    for (const property of keys) {
      const from = baseline[property] ?? ""
      const to = styleDraft[property] ?? ""
      if (from.trim() === to.trim()) continue
      if (!to.trim() && !from.trim()) continue
      changes.push({ property, from, to })
    }
    return changes
  }

  const onSave = () => {
    if (!detail) {
      pushToast("Select a component first", "error")
      return
    }
    const changes = dirtyChanges()
    if (changes.length === 0) {
      pushToast("No CSS changes to save", "error")
      return
    }
    const payload: ComponentStyleEditPayload = {
      componentName: detail.name,
      file: detail.file,
      exportName: detail.exportName,
      targetSelector: domTarget?.selector ?? null,
      propsSummary: detail.props.map((p) =>
        `${p.name}${p.optional ? "?" : ""}${p.typeHint ? `: ${p.typeHint}` : ""}`,
      ),
      changes,
    }
    addAttachment({
      id: `${Date.now()}-component-style`,
      kind: "component-style",
      name: `${detail.name} styles`,
      payload,
    })
    window.dispatchEvent(new CustomEvent("flex:focus-composer"))
    pushToast("Style edit attached — send to apply via the agent", "success")
    setBaseline({ ...styleDraft })
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
          description="Pick a working directory for this session to discover React components."
        />
      </div>
    )
  }

  if (detect && !detect.isReact) {
    return (
      <div className="absolute inset-0 flex flex-col">
        <EmptyState
          className="min-h-0 flex-1"
          title="No React app detected"
          description={detect.reason}
        />
      </div>
    )
  }

  const busy = detectFetching || listFetching

  return (
    <div className="absolute inset-0 flex flex-col">
      <div className="flex h-6 shrink-0 items-center gap-1 border-b border-stroke-3 px-2.5">
        <span className="min-w-0 flex-1 truncate text-xs text-ink-muted">
          {list
            ? `${list.components.length} component${list.components.length === 1 ? "" : "s"}`
            : busy
              ? "Scanning…"
              : "Components"}
        </span>
        <IconButton
          label="Refresh"
          quiet
          className="h-5 w-5"
          onClick={() => {
            void refetchList().catch((err) => setError(toInvokeError(err)))
          }}
        >
          <RefreshCw className={cn("h-3 w-3", busy && "animate-spin")} aria-hidden />
        </IconButton>
      </div>

      {error ? (
        <p className="shrink-0 border-b border-stroke-3 px-2.5 py-1.5 text-xs text-danger">
          {error}
        </p>
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
          <aside className="flex w-[180px] shrink-0 flex-col border-r border-stroke-3">
            <ScrollArea className="min-h-0 flex-1 py-1.5">
              <ul>
                {tree.map(({ node, depth }) => {
                  const isActive = node.id === selectedId
                  return (
                    <li key={node.id}>
                      <button
                        type="button"
                        onClick={() => setSelectedId(node.id)}
                        style={{ paddingLeft: `${10 + depth * 10}px` }}
                        className={cn(
                          "flex w-full items-center gap-1 py-1.5 pr-2.5 text-left text-xs",
                          isActive
                            ? "bg-fill-2 text-ink"
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
                      </button>
                    </li>
                  )
                })}
              </ul>
            </ScrollArea>
          </aside>

          <main className="flex min-w-0 flex-1 flex-col">
            {detail ? (
              <ComponentDetailPane
                detail={detail}
                styleDraft={styleDraft}
                hasLiveTarget={!!domTarget}
                onSetProp={setProp}
                onSave={onSave}
                onOpenBrowser={openBrowserHint}
              />
            ) : (
              <EmptyState
                className="min-h-0 flex-1"
                title="Select a component"
                description="Pick a component from the tree to inspect props and edit CSS."
              />
            )}
          </main>
        </div>
      )}
    </div>
  )
}

type DetailPaneProps = {
  detail: ComponentDetail
  styleDraft: StyleDraft
  hasLiveTarget: boolean
  onSetProp: (property: string, value: string) => void
  onSave: () => void
  onOpenBrowser: () => void
}

const ComponentDetailPane = ({
  detail,
  styleDraft,
  hasLiveTarget,
  onSetProp,
  onSave,
  onOpenBrowser,
}: DetailPaneProps) => {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
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
          minHeight: 120,
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
        </div>
        {!hasLiveTarget ? (
          <button
            type="button"
            onClick={onOpenBrowser}
            className="inline-flex items-center gap-1 text-xs text-ink-muted hover:text-ink"
          >
            <ExternalLink className="h-3 w-3" aria-hidden />
            Open Browser + Design Mode for live preview
          </button>
        ) : (
          <span className="text-xs text-ink-muted">
            Live overrides apply to the Design Mode selection
          </span>
        )}
      </div>

      <ScrollArea className="min-h-0 flex-1">
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

          <section>
            <div className="mb-1 flex items-center justify-between gap-2">
              <h3 className="text-xs font-medium text-ink-muted">CSS</h3>
              <Button size="sm" onClick={onSave} className="h-6 gap-1 px-2 text-xs">
                <Save className="h-3 w-3" aria-hidden />
                Save
              </Button>
            </div>
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

          {detail.sourceSnippet ? (
            <section>
              <h3 className="mb-1 text-xs font-medium text-ink-muted">Source</h3>
              <pre className="overflow-x-auto rounded-md border border-stroke-3 bg-fill-3 p-2 font-mono text-[11px] leading-snug text-ink-secondary">
                {detail.sourceSnippet}
              </pre>
            </section>
          ) : null}
        </div>
      </ScrollArea>
    </div>
  )
}

import { useRef, useState } from "react"
import { Download, Pencil, Plus, Trash2, Upload } from "lucide-react"
import { useAppStore } from "../../stores/appStore"
import { THEME_TOKEN_ALLOWLIST } from "../../lib/themeTokens"
import type { ThemeSpec } from "../../lib/themeTokens"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

const EDITOR_TOKENS: Array<{ key: (typeof THEME_TOKEN_ALLOWLIST)[number]; label: string }> = [
  { key: "--color-chrome", label: "Chrome (main surface)" },
  { key: "--color-panel", label: "Panel (sidebar / popovers)" },
  { key: "--color-elevated", label: "Elevated (bubbles / tiles)" },
  { key: "--color-text-1", label: "Text primary" },
  { key: "--color-text-2", label: "Text secondary" },
  { key: "--color-fill-2", label: "Fill selected" },
  { key: "--color-fill-4", label: "Fill hover" },
  { key: "--color-stroke-3", label: "Stroke hairline" },
  { key: "--color-accent", label: "Accent" },
  { key: "--color-accent-text", label: "Accent text" },
]

const FACTORY_ID = "factory"

const slugify = (name: string): string =>
  name
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "") || "custom"

type EditorState = {
  id: string
  name: string
  tokens: Record<string, string>
}

const emptyEditor = (): EditorState => ({
  id: "",
  name: "",
  tokens: {},
})

const editorFromSpec = (spec: ThemeSpec, mode: "dark" | "light"): EditorState => ({
  id: spec.id,
  name: spec.name,
  tokens: { ...(spec.tokens?.[mode] ?? {}) },
})

const applyEditorToSpec = (
  editor: EditorState,
  existing: ThemeSpec | undefined,
  mode: "dark" | "light",
): ThemeSpec => {
  const tokens = { ...existing?.tokens }
  const filtered: Record<string, string> = {}
  for (const { key } of EDITOR_TOKENS) {
    if (editor.tokens[key]) filtered[key] = editor.tokens[key]
  }
  tokens[mode] = Object.keys(filtered).length > 0 ? filtered : undefined
  return {
    version: 1,
    id: editor.id || slugify(editor.name),
    name: editor.name,
    ...(existing?.base ? { base: existing.base } : {}),
    ...(Object.keys(tokens).some((k) => tokens[k as keyof typeof tokens] !== undefined)
      ? { tokens: tokens as ThemeSpec["tokens"] }
      : {}),
  }
}

type ThemeEditorDialogProps = {
  open: boolean
  onOpenChange: (open: boolean) => void
  initialSpec: ThemeSpec | null
  mode: "dark" | "light"
  onSave: (spec: ThemeSpec) => void
}

const ThemeEditorDialog = ({
  open,
  onOpenChange,
  initialSpec,
  mode,
  onSave,
}: ThemeEditorDialogProps) => {
  const [editor, setEditor] = useState<EditorState>(() =>
    initialSpec ? editorFromSpec(initialSpec, mode) : emptyEditor(),
  )

  const prevOpenRef = useRef(open)
  if (open !== prevOpenRef.current) {
    prevOpenRef.current = open
    if (open) {
      setEditor(initialSpec ? editorFromSpec(initialSpec, mode) : emptyEditor())
    }
  }

  const setToken = (key: string, value: string) => {
    setEditor((prev) => ({ ...prev, tokens: { ...prev.tokens, [key]: value } }))
  }

  const handleSave = () => {
    if (!editor.name.trim()) return
    const id = editor.id || slugify(editor.name)
    const spec = applyEditorToSpec({ ...editor, id }, initialSpec ?? undefined, mode)
    onSave(spec)
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm" showCloseButton>
        <DialogHeader>
          <DialogTitle>{initialSpec ? "Edit theme" : "New theme"}</DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-3 py-2">
          <div className="flex flex-col gap-1">
            <label className="text-xs text-ink-secondary" htmlFor="theme-name">
              Name
            </label>
            <Input
              id="theme-name"
              value={editor.name}
              onChange={(e) => {
                const name = e.target.value
                setEditor((prev) => ({
                  ...prev,
                  name,
                  id: initialSpec ? prev.id : slugify(name),
                }))
              }}
              placeholder="My Theme"
              className="h-7 text-xs"
              aria-label="Theme name"
            />
          </div>

          <div className="flex flex-col gap-1">
            <label className="text-xs text-ink-secondary" htmlFor="theme-id">
              ID (slug)
            </label>
            <Input
              id="theme-id"
              value={editor.id}
              onChange={(e) =>
                setEditor((prev) => ({ ...prev, id: e.target.value }))
              }
              placeholder={slugify(editor.name) || "my-theme"}
              className="h-7 font-mono text-xs"
              aria-label="Theme id slug"
            />
          </div>

          <div className="flex flex-col gap-1.5">
            <p className="text-xs text-ink-secondary">
              Token overrides{" "}
              <span className="text-ink-muted">({mode} mode)</span>
            </p>
            <div className="flex flex-col gap-1">
              {EDITOR_TOKENS.map(({ key, label }) => (
                <div key={key} className="flex items-center gap-2">
                  <input
                    type="color"
                    value={editor.tokens[key] ?? "#000000"}
                    onChange={(e) => setToken(key, e.target.value)}
                    className="h-6 w-6 cursor-pointer rounded border border-stroke-2 bg-transparent p-0.5"
                    aria-label={label}
                    title={label}
                  />
                  <span className="min-w-0 flex-1 truncate text-xs text-ink">
                    {label}
                  </span>
                  <span className="shrink-0 font-mono text-[10px] text-ink-muted">
                    {editor.tokens[key] ?? "—"}
                  </span>
                  {editor.tokens[key] ? (
                    <button
                      type="button"
                      onClick={() => {
                        const { [key]: _removed, ...rest } = editor.tokens
                        setEditor((prev) => ({ ...prev, tokens: rest }))
                      }}
                      className="shrink-0 text-ink-muted hover:text-ink"
                      aria-label={`Clear ${label}`}
                    >
                      ×
                    </button>
                  ) : null}
                </div>
              ))}
            </div>
          </div>
        </div>

        <div className="flex justify-end gap-2 pt-1">
          <Button
            variant="ghost"
            size="xs"
            onClick={() => onOpenChange(false)}
          >
            Cancel
          </Button>
          <Button
            size="xs"
            onClick={handleSave}
            disabled={!editor.name.trim()}
          >
            Save
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

export const ThemeLibrary = () => {
  const theme = useAppStore((s) => s.theme)
  const activeThemeId = useAppStore((s) => s.activeThemeId)
  const customThemes = useAppStore((s) => s.customThemes)
  const setActiveTheme = useAppStore((s) => s.setActiveTheme)
  const upsertCustomTheme = useAppStore((s) => s.upsertCustomTheme)
  const deleteCustomTheme = useAppStore((s) => s.deleteCustomTheme)
  const importThemeJson = useAppStore((s) => s.importThemeJson)
  const pushToast = useAppStore((s) => s.pushToast)

  const [editorOpen, setEditorOpen] = useState(false)
  const [editingSpec, setEditingSpec] = useState<ThemeSpec | null>(null)

  const fileInputRef = useRef<HTMLInputElement>(null)

  const activeSpec =
    activeThemeId === FACTORY_ID
      ? null
      : (customThemes.find((t) => t.id === activeThemeId) ?? null)

  const handleImport = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    const reader = new FileReader()
    reader.onload = (ev) => {
      const raw = ev.target?.result
      if (typeof raw !== "string") return
      const result = importThemeJson(raw)
      if (!result.ok) {
        pushToast(`Import failed: ${result.errors.join(", ")}`, "error")
        return
      }
      if (result.skipped.length > 0) {
        pushToast(
          `Theme imported. Skipped unknown tokens: ${result.skipped.slice(0, 5).join(", ")}${result.skipped.length > 5 ? ` and ${result.skipped.length - 5} more` : ""}`,
          "success",
        )
      } else {
        pushToast(`Theme "${result.spec.name}" imported`, "success")
      }
    }
    reader.readAsText(file)
    e.target.value = ""
  }

  const handleExport = () => {
    const spec = activeSpec
    if (!spec) return
    const json = JSON.stringify(spec, null, 2)
    const blob = new Blob([json], { type: "application/json" })
    const url = URL.createObjectURL(blob)
    const a = document.createElement("a")
    a.href = url
    a.download = `${spec.id}.theme.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  const handleNewTheme = () => {
    setEditingSpec(null)
    setEditorOpen(true)
  }

  const handleEditTheme = () => {
    if (!activeSpec) return
    setEditingSpec(activeSpec)
    setEditorOpen(true)
  }

  const handleDelete = () => {
    if (!activeSpec) return
    deleteCustomTheme(activeSpec.id)
    pushToast(`Theme "${activeSpec.name}" deleted`, "success")
  }

  const handleEditorSave = (spec: ThemeSpec) => {
    upsertCustomTheme(spec)
    setActiveTheme(spec.id)
    pushToast(`Theme "${spec.name}" saved`, "success")
  }

  return (
    <>
      <div className="flex w-full max-w-md flex-col gap-2" data-settings-row="appearance-theme-library">
        <div className="flex items-center gap-2">
          <Select
            value={activeThemeId}
            onValueChange={(v) => {
              if (v) setActiveTheme(v)
            }}
          >
            <SelectTrigger
              className="h-7 flex-1 text-xs"
              size="sm"
              aria-label="Active theme"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                <SelectItem value={FACTORY_ID}>Factory (built-in)</SelectItem>
                {customThemes.map((t) => (
                  <SelectItem key={t.id} value={t.id}>
                    {t.name}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>

          <Button
            variant="ghost"
            size="icon-xs"
            title="New theme"
            aria-label="New theme"
            onClick={handleNewTheme}
          >
            <Plus className="h-3.5 w-3.5" aria-hidden />
          </Button>

          {activeSpec ? (
            <>
              <Button
                variant="ghost"
                size="icon-xs"
                title="Edit theme"
                aria-label="Edit theme"
                onClick={handleEditTheme}
              >
                <Pencil className="h-3.5 w-3.5" aria-hidden />
              </Button>
              <Button
                variant="ghost"
                size="icon-xs"
                title="Export theme as JSON"
                aria-label="Export theme as JSON"
                onClick={handleExport}
              >
                <Download className="h-3.5 w-3.5" aria-hidden />
              </Button>
              <Button
                variant="ghost"
                size="icon-xs"
                title="Delete theme"
                aria-label="Delete theme"
                onClick={handleDelete}
                className={cn("hover:text-red-500")}
              >
                <Trash2 className="h-3.5 w-3.5" aria-hidden />
              </Button>
            </>
          ) : null}

          <Button
            variant="ghost"
            size="icon-xs"
            title="Import theme from JSON"
            aria-label="Import theme from JSON"
            onClick={() => fileInputRef.current?.click()}
          >
            <Upload className="h-3.5 w-3.5" aria-hidden />
          </Button>
        </div>

        {activeSpec ? (
          <p className="text-[11px] leading-4 text-ink-muted">
            Custom token overrides applied on top of the{" "}
            <span className="text-ink-secondary">{theme}</span> factory palette.
          </p>
        ) : (
          <p className="text-[11px] leading-4 text-ink-muted">
            Factory — using the built-in {theme} palette.
          </p>
        )}

        <input
          ref={fileInputRef}
          type="file"
          accept=".json,application/json"
          className="sr-only"
          onChange={handleImport}
          aria-hidden
          tabIndex={-1}
        />
      </div>

      <ThemeEditorDialog
        open={editorOpen}
        onOpenChange={setEditorOpen}
        initialSpec={editingSpec}
        mode={theme}
        onSave={handleEditorSave}
      />
    </>
  )
}

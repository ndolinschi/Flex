/** Monaco worker + language wiring for Vite / Tauri.
 *
 * Call once before mounting an editor. Uses Vite `?worker` imports so
 * workers load from the same origin as the app (CDN is unreliable offline).
 * Import this module dynamically — static imports pull ~3MB into the graph. */
import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker"
import cssWorker from "monaco-editor/esm/vs/language/css/css.worker?worker"
import htmlWorker from "monaco-editor/esm/vs/language/html/html.worker?worker"
import jsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker"
import tsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker"
import { loader } from "@monaco-editor/react"
import * as monaco from "monaco-editor"
import {
  INLINE_COMPLETION_DEBOUNCE_MS,
  INLINE_COMPLETION_MIN_PREFIX,
} from "./inlineCompletion"
import { completePromptInline } from "./tauri"

export { languageForPath } from "./monacoLanguages"

let ready = false

/** Module-level flag updated by FilesTab via `setMonacoCompletionEnabled`. */
const completionState = { enabled: false }

/** Called by FilesTab when inline-completion prefs change. */
export const setMonacoCompletionEnabled = (enabled: boolean): void => {
  completionState.enabled = enabled
}

/** Singleton disposable — provider registered at most once per page load. */
let inlineProviderDisposable: monaco.IDisposable | null = null

/** Register the inline-completions provider (idempotent). */
const ensureInlineProvider = (): void => {
  if (inlineProviderDisposable) return
  inlineProviderDisposable = monaco.languages.registerInlineCompletionsProvider(
    { pattern: "**" },
    {
      provideInlineCompletions: async (model, position, _context, token) => {
        if (!completionState.enabled) return { items: [] }

        const prefix = model.getValueInRange({
          startLineNumber: 1,
          startColumn: 1,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        })
        if (prefix.trim().length < INLINE_COMPLETION_MIN_PREFIX) {
          return { items: [] }
        }
        const suffix = model.getValueInRange({
          startLineNumber: position.lineNumber,
          startColumn: position.column,
          endLineNumber: model.getLineCount(),
          endColumn: model.getLineMaxColumn(model.getLineCount()),
        })

        // Debounce — resolve early when Monaco cancels the token.
        await new Promise<void>((resolve) => {
          const timer = setTimeout(resolve, INLINE_COMPLETION_DEBOUNCE_MS)
          token.onCancellationRequested(() => {
            clearTimeout(timer)
            resolve()
          })
        })
        if (token.isCancellationRequested) return { items: [] }

        try {
          const text = await completePromptInline(prefix, suffix)
          if (token.isCancellationRequested || !text) return { items: [] }
          return { items: [{ insertText: text }] }
        } catch {
          return { items: [] }
        }
      },
      freeInlineCompletions: () => {},
    },
  )
}

export const ensureMonaco = (): void => {
  if (ready || typeof window === "undefined") return
  ready = true
  self.MonacoEnvironment = {
    getWorker(_workerId: string, label: string) {
      if (label === "json") return new jsonWorker()
      if (label === "css" || label === "scss" || label === "less") {
        return new cssWorker()
      }
      if (label === "html" || label === "handlebars" || label === "razor") {
        return new htmlWorker()
      }
      if (label === "typescript" || label === "javascript") {
        return new tsWorker()
      }
      return new editorWorker()
    },
  }
  loader.config({ monaco })

  // TypeScript / JavaScript: ESNext target, React JSX, syntax + semantic diagnostics.
  const tsCompilerOptions: monaco.languages.typescript.CompilerOptions = {
    target: monaco.languages.typescript.ScriptTarget.ESNext,
    module: monaco.languages.typescript.ModuleKind.ESNext,
    moduleResolution:
      monaco.languages.typescript.ModuleResolutionKind.NodeJs,
    jsx: monaco.languages.typescript.JsxEmit.ReactJSX,
    noEmit: true,
    allowJs: true,
    allowSyntheticDefaultImports: true,
    esModuleInterop: true,
  }
  const tsDiagOptions: monaco.languages.typescript.DiagnosticsOptions = {
    noSemanticValidation: false,
    noSyntaxValidation: false,
  }
  monaco.languages.typescript.typescriptDefaults.setCompilerOptions(
    tsCompilerOptions,
  )
  monaco.languages.typescript.typescriptDefaults.setDiagnosticsOptions(
    tsDiagOptions,
  )
  monaco.languages.typescript.javascriptDefaults.setCompilerOptions(
    tsCompilerOptions,
  )
  monaco.languages.typescript.javascriptDefaults.setDiagnosticsOptions(
    tsDiagOptions,
  )

  ensureInlineProvider()
}

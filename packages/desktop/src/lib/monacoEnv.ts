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

export { languageForPath } from "./monacoLanguages"

let ready = false

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
}

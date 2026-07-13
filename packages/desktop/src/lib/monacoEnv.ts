/** Monaco worker + language wiring for Vite / Tauri.
 *
 * Call once before mounting an editor. Uses Vite `?worker` imports so
 * workers load from the same origin as the app (CDN is unreliable offline). */
import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker"
import cssWorker from "monaco-editor/esm/vs/language/css/css.worker?worker"
import htmlWorker from "monaco-editor/esm/vs/language/html/html.worker?worker"
import jsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker"
import tsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker"
import { loader } from "@monaco-editor/react"
import * as monaco from "monaco-editor"

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

/** Best-effort Monaco language id from a repo-relative path. */
export const languageForPath = (path: string): string => {
  const name = path.split("/").pop()?.toLowerCase() ?? ""
  const ext = name.includes(".") ? name.slice(name.lastIndexOf(".") + 1) : name
  switch (ext) {
    case "ts":
    case "mts":
    case "cts":
      return "typescript"
    case "tsx":
      return "typescript"
    case "js":
    case "mjs":
    case "cjs":
      return "javascript"
    case "jsx":
      return "javascript"
    case "json":
      return "json"
    case "css":
      return "css"
    case "scss":
      return "scss"
    case "less":
      return "less"
    case "html":
    case "htm":
      return "html"
    case "md":
    case "mdx":
      return "markdown"
    case "rs":
      return "rust"
    case "py":
      return "python"
    case "toml":
      return "ini"
    case "yml":
    case "yaml":
      return "yaml"
    case "sh":
    case "bash":
    case "zsh":
      return "shell"
    case "sql":
      return "sql"
    case "xml":
    case "svg":
      return "xml"
    case "go":
      return "go"
    case "java":
      return "java"
    case "kt":
      return "kotlin"
    case "swift":
      return "swift"
    case "c":
    case "h":
      return "c"
    case "cpp":
    case "cc":
    case "hpp":
      return "cpp"
    case "rb":
      return "ruby"
    case "php":
      return "php"
    case "dockerfile":
      return "dockerfile"
    default:
      if (name === "dockerfile") return "dockerfile"
      if (name === "makefile") return "plaintext"
      return "plaintext"
  }
}

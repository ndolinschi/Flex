/** Subscribe to Monaco editor marker changes for a given model URI.
 *
 * Monaco's TS/JS/JSON/CSS workers push diagnostics as markers; this hook
 * surfaces them so the FilesTab Problems strip can display them. Import this
 * module dynamically — it pulls the full monaco-editor namespace lazily.
 *
 * Accepts a string URI (the `path` prop passed to `@monaco-editor/react`);
 * internally resolves the Monaco `Uri` after the worker module loads. */

import { useEffect, useRef, useState } from "react"
import type * as MonacoNs from "monaco-editor"

export type MonacoMarker = MonacoNs.editor.IMarker

/** Severity constants mirroring `monaco.MarkerSeverity` (avoids a static import). */
export const MarkerSeverity = {
  Hint: 1,
  Info: 2,
  Warning: 4,
  Error: 8,
} as const

/**
 * Returns Monaco markers for the model identified by `modelPath` (the `path`
 * prop of `@monaco-editor/react`). Returns `[]` when the model or monaco
 * namespace is not yet loaded.
 */
export const useMonacoMarkers = (modelPath: string | null): MonacoMarker[] => {
  const [markers, setMarkers] = useState<MonacoMarker[]>([])
  // Keep the disposable stable across re-renders without triggering effects.
  const disposeRef = useRef<MonacoNs.IDisposable | null>(null)
  const monacoRef = useRef<typeof MonacoNs | null>(null)

  useEffect(() => {
    if (!modelPath) {
      setMarkers([])
      return
    }

    let cancelled = false

    void import("monaco-editor").then((monaco) => {
      if (cancelled) return
      monacoRef.current = monaco

      const refresh = (uri: MonacoNs.Uri) => {
        setMarkers(monaco.editor.getModelMarkers({ resource: uri }))
      }

      const uri = monaco.Uri.parse(modelPath)
      refresh(uri)

      disposeRef.current?.dispose()
      disposeRef.current = monaco.editor.onDidChangeMarkers((uris) => {
        if (uris.some((u) => u.toString() === uri.toString())) {
          refresh(uri)
        }
      })
    })

    return () => {
      cancelled = true
      disposeRef.current?.dispose()
      disposeRef.current = null
    }
  }, [modelPath])

  return markers
}

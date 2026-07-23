
import { useEffect, useRef, useState } from "react"
import type * as MonacoNs from "monaco-editor"
import { shouldSubscribeMonacoMarkers } from "../lib/monacoPolicy"

export type MonacoMarker = MonacoNs.editor.IMarker

export const MarkerSeverity = {
  Hint: 1,
  Info: 2,
  Warning: 4,
  Error: 8,
} as const

export const useMonacoMarkers = (
  modelPath: string | null,
  enabled = true,
): MonacoMarker[] => {
  const [markers, setMarkers] = useState<MonacoMarker[]>([])
  const disposeRef = useRef<MonacoNs.IDisposable | null>(null)
  const monacoRef = useRef<typeof MonacoNs | null>(null)

  useEffect(() => {
    if (!shouldSubscribeMonacoMarkers(modelPath, enabled) || !modelPath) {
      setMarkers([])
      disposeRef.current?.dispose()
      disposeRef.current = null
      return
    }

    const path = modelPath
    let cancelled = false

    void import("monaco-editor").then((monaco) => {
      if (cancelled) return
      monacoRef.current = monaco

      const refresh = (uri: MonacoNs.Uri) => {
        setMarkers(monaco.editor.getModelMarkers({ resource: uri }))
      }

      const uri = monaco.Uri.parse(path)
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
  }, [modelPath, enabled])

  return markers
}

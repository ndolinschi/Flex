import { useCallback, useEffect, useState } from "react"
import { ActivityBar } from "./ActivityBar"
import { AiPanel } from "./AiPanel"
import { BottomPanel } from "./BottomPanel"
import { CommandPaletteStub } from "./CommandPaletteStub"
import { Editor } from "./Editor"
import { Sidebar } from "./Sidebar"
import { StatusBar } from "./StatusBar"
import "./ide-mock.css"

const PLAYGROUND_HASHES = new Set(["#ide-playground", "#playground"])

export const isIdePlaygroundHash = (hash = window.location.hash): boolean =>
  PLAYGROUND_HASHES.has(hash)

export const IdePlayground = () => {
  const [orientation, setOrientation] = useState<"horizontal" | "vertical">(
    "horizontal",
  )
  const [activity, setActivity] = useState("explorer")
  const [activeFile, setActiveFile] = useState("Composer.tsx")
  const [openTabs, setOpenTabs] = useState(["Composer.tsx", "ChatShell.tsx"])
  const [bottomOpen, setBottomOpen] = useState(true)
  const [paletteOpen, setPaletteOpen] = useState(false)

  const openFile = useCallback((name: string) => {
    setActiveFile(name)
    setOpenTabs((tabs) => (tabs.includes(name) ? tabs : [...tabs, name]))
  }, [])

  const closeTab = useCallback(
    (name: string) => {
      setOpenTabs((tabs) => {
        const next = tabs.filter((t) => t !== name)
        if (name === activeFile) {
          setActiveFile(next[next.length - 1] ?? "Composer.tsx")
        }
        return next.length > 0 ? next : ["Composer.tsx"]
      })
    },
    [activeFile],
  )

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey
      if (meta && e.key.toLowerCase() === "k") {
        e.preventDefault()
        setPaletteOpen((v) => !v)
      }
      if (meta && e.key === "`") {
        e.preventDefault()
        setBottomOpen((v) => !v)
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [])

  const activityBar = (
    <ActivityBar
      orientation={orientation}
      active={activity}
      onSelect={setActivity}
      onToggleOrientation={() =>
        setOrientation((o) => (o === "horizontal" ? "vertical" : "horizontal"))
      }
    />
  )

  return (
    <div className="ide-mock-playground relative flex flex-col" data-demo="ide-playground">
      <div className="pointer-events-none absolute top-2 right-2 z-40 rounded-[var(--radius-chrome)] border border-[var(--border)] bg-[var(--bg-elevated)] px-2 py-1 text-[10px] text-[var(--text-secondary)]">
        Demo playground · factory Flex theme unchanged ·{" "}
        <span className="text-[var(--text-muted)]">#ide-playground</span>
      </div>

      {orientation === "horizontal" ? activityBar : null}

      <div className="flex min-h-0 flex-1">
        {orientation === "vertical" ? activityBar : null}
        <Sidebar activeFile={activeFile} onOpenFile={openFile} />
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex min-h-0 flex-1">
            <Editor
              openTabs={openTabs}
              activeFile={activeFile}
              onSelectTab={setActiveFile}
              onCloseTab={closeTab}
            />
            <AiPanel />
          </div>
          <BottomPanel open={bottomOpen} onToggle={() => setBottomOpen((v) => !v)} />
        </div>
      </div>

      <StatusBar file={activeFile} orientation={orientation} />
      <CommandPaletteStub open={paletteOpen} onClose={() => setPaletteOpen(false)} />
    </div>
  )
}

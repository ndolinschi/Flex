import { describe, expect, it, vi } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import {
  FileCode2,
  GitBranch,
  Globe,
  Terminal as TerminalIcon,
} from "lucide-react"
import { RightPanelMiniTabs } from "./RightPanelMiniTabs"
import { PROJECT_PINNED_TABS, type RightPanelTabDef } from "./tabs"

const catalog: RightPanelTabDef[] = [
  { id: "changes", label: "Changes", icon: GitBranch },
  { id: "files", label: "Files", icon: FileCode2 },
  { id: "terminal", label: "Terminal", icon: TerminalIcon },
  { id: "browser", label: "Browser", icon: Globe },
]

describe("PROJECT_PINNED_TABS", () => {
  it("pins Changes, Browser, Terminal, Files in Cursor order", () => {
    expect(PROJECT_PINNED_TABS).toEqual([
      "changes",
      "browser",
      "terminal",
      "files",
    ])
  })
})

describe("RightPanelMiniTabs", () => {
  it("renders rows with DiffStat and terminal count, without section chrome", () => {
    const html = renderToStaticMarkup(
      <RightPanelMiniTabs
        openTabDefs={[catalog[2]!]}
        selectedTab="terminal"
        changesTotals={{ added: 66, removed: 57 }}
        terminalCount={1}
        catalog={catalog}
        onSelectTab={() => undefined}
      />,
    )
    expect(html).not.toContain("Open Tabs")
    expect(html).not.toContain("On ")
    expect(html).not.toContain("Show panel")
    expect(html).not.toContain("border-stroke-3")
    expect(html).toContain("1 Terminal")
    expect(html).toContain("+66")
    expect(html).toContain("Changes")
    expect(html).toContain("Browser")
    expect(html).toContain("Files")
  })

  it("dedupes open tabs from the pinned list", () => {
    const html = renderToStaticMarkup(
      <RightPanelMiniTabs
        openTabDefs={[catalog[0]!]}
        selectedTab="changes"
        changesTotals={{ added: 1, removed: 0 }}
        terminalCount={0}
        catalog={catalog}
        onSelectTab={() => undefined}
      />,
    )
    // Changes appears once (open), not again in pinned.
    expect(html.split(">Changes<").length - 1).toBe(1)
  })

  it("accepts onSelectTab without expand control", () => {
    const onSelectTab = vi.fn()
    const html = renderToStaticMarkup(
      <RightPanelMiniTabs
        openTabDefs={[]}
        selectedTab="plan"
        changesTotals={{ added: 0, removed: 0 }}
        terminalCount={0}
        catalog={catalog}
        onSelectTab={onSelectTab}
      />,
    )
    expect(html).toContain("Details panel shortcuts")
    expect(html).not.toContain('aria-label="Show panel"')
    expect(onSelectTab).not.toHaveBeenCalled()
  })
})

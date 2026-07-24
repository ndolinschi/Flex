import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import {
  PanelToolbar,
  PanelToolbarTitle,
  panelChromeIconActiveClass,
  panelChromeIconClass,
} from "./PanelToolbar"
import { PanelSideRail } from "./PanelSideRail"

describe("PanelToolbar", () => {
  it("defaults to host chrome (30px, border-b, px-2.5)", () => {
    const html = renderToStaticMarkup(
      <PanelToolbar>
        <PanelToolbarTitle>Browser</PanelToolbarTitle>
      </PanelToolbar>,
    )
    expect(html).toContain("border-b")
    expect(html).toContain("border-stroke-3")
    expect(html).toContain("px-2.5")
    expect(html).toContain("--header-height")
    expect(html).toContain('role="toolbar"')
  })

  it("elevated variant uses panel-toolbar recipe height", () => {
    const html = renderToStaticMarkup(
      <PanelToolbar variant="elevated" actions={<button type="button">+</button>}>
        <span>Terminal</span>
      </PanelToolbar>,
    )
    expect(html).toContain("panel-toolbar")
    expect(html).toContain("--panel-toolbar-height")
    expect(html).toContain("ml-auto")
  })

  it("quiet variant omits border-b", () => {
    const html = renderToStaticMarkup(
      <PanelToolbar variant="quiet">
        <PanelToolbarTitle>Status</PanelToolbarTitle>
      </PanelToolbar>,
    )
    expect(html).not.toMatch(/class="[^"]*border-b/)
  })

  it("exports shared icon button classes", () => {
    expect(panelChromeIconClass).toContain("hover:bg-fill-4")
    expect(panelChromeIconActiveClass).toContain("bg-fill-2")
  })
})

describe("PanelSideRail", () => {
  it("defaults to 180px and renders optional header", () => {
    const html = renderToStaticMarkup(
      <PanelSideRail header="3 items">
        <ul />
      </PanelSideRail>,
    )
    expect(html).toContain("w-[180px]")
    expect(html).toContain("border-r")
    expect(html).toContain("3 items")
  })

  it("supports Terminal 160px width", () => {
    const html = renderToStaticMarkup(
      <PanelSideRail width={160}>
        <ul />
      </PanelSideRail>,
    )
    expect(html).toContain("w-[160px]")
  })
})

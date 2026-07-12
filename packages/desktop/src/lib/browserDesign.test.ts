import { describe, expect, it } from "vitest"
import {
  formatDomContextMarkdown,
  mergeDomContextWithDraft,
  type BrowserDomElement,
} from "./browserDesign"

const sampleElement = (
  overrides: Partial<BrowserDomElement> = {},
): BrowserDomElement => ({
  url: "http://localhost:3000/app",
  tag: "button",
  id: "save",
  classes: "btn primary",
  selector: "#save",
  xpath: "/html/body/button",
  attributes: { type: "submit", "data-testid": "save" },
  outerHtml: '<button id="save" class="btn primary" type="submit">Save</button>',
  styles: { display: "inline-block", color: "rgb(0, 0, 0)" },
  rect: { x: 10, y: 20, width: 80, height: 32 },
  ...overrides,
})

describe("formatDomContextMarkdown", () => {
  it("returns empty for no attachments", () => {
    expect(formatDomContextMarkdown([])).toBe("")
  })

  it("includes selector, url, and html snippet", () => {
    const md = formatDomContextMarkdown([
      { name: "button#save", payload: sampleElement() },
    ])
    expect(md).toContain("## Selected page elements")
    expect(md).toContain("button#save")
    expect(md).toContain("http://localhost:3000/app")
    expect(md).toContain("`#save`")
    expect(md).toContain("```html")
    expect(md).toContain('id="save"')
  })

  it("numbers multiple elements", () => {
    const md = formatDomContextMarkdown([
      { name: "a", payload: sampleElement({ selector: "#a" }) },
      { name: "b", payload: sampleElement({ selector: "#b", tag: "a" }) },
    ])
    expect(md).toContain("### Element 1:")
    expect(md).toContain("### Element 2:")
  })

  it("omits empty attribute/style sections gracefully", () => {
    const md = formatDomContextMarkdown([
      {
        name: "<div>",
        payload: sampleElement({
          attributes: {},
          styles: { display: "none" },
          outerHtml: "",
        }),
      },
    ])
    expect(md).toContain("### Element 1:")
    expect(md).not.toContain("Attributes:")
    expect(md).not.toContain("```html")
  })
})

describe("mergeDomContextWithDraft", () => {
  it("returns draft alone when no dom chips", () => {
    expect(mergeDomContextWithDraft("fix the button", [])).toBe(
      "fix the button",
    )
  })

  it("returns context alone when draft is empty", () => {
    const out = mergeDomContextWithDraft("", [
      { name: "button#save", payload: sampleElement() },
    ])
    expect(out).toContain("## Selected page elements")
    expect(out).not.toContain("---")
  })

  it("joins context and draft with a separator", () => {
    const out = mergeDomContextWithDraft("Make this primary blue", [
      { name: "button#save", payload: sampleElement() },
    ])
    expect(out).toContain("## Selected page elements")
    expect(out).toContain("---")
    expect(out).toContain("Make this primary blue")
  })
})

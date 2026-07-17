import { describe, expect, it } from "vitest"
import {
  formatComponentStyleMarkdown,
  mergeComponentStyleWithDraft,
  parseComponentStyleMessage,
  type ComponentStyleEditPayload,
} from "./componentDesign"

const sample = (
  overrides: Partial<ComponentStyleEditPayload> = {},
): ComponentStyleEditPayload => ({
  componentName: "Button",
  file: "src/components/Button.tsx",
  exportName: "Button",
  targetSelector: "button.primary",
  propsSummary: ["label: string", "disabled?: boolean"],
  changes: [
    { property: "background-color", from: "#fff", to: "#1a1a1a" },
    { property: "border-radius", from: "4px", to: "12px" },
  ],
  ...overrides,
})

describe("formatComponentStyleMarkdown", () => {
  it("includes component path and property diffs", () => {
    const md = formatComponentStyleMarkdown(sample())
    expect(md).toContain("## Component style edit")
    expect(md).toContain("Button (src/components/Button.tsx)")
    expect(md).toContain("background-color: #fff → #1a1a1a")
    expect(md).toContain("border-radius: 4px → 12px")
    expect(md).toContain("button.primary")
  })
})

describe("mergeComponentStyleWithDraft", () => {
  it("returns draft alone when no payloads", () => {
    expect(mergeComponentStyleWithDraft("fix it", [])).toBe("fix it")
  })

  it("merges context before the instruction", () => {
    const out = mergeComponentStyleWithDraft("Make it darker", [sample()])
    expect(out.startsWith("## Component style edit")).toBe(true)
    expect(out).toContain("Make it darker")
  })
})

describe("parseComponentStyleMessage", () => {
  it("returns null for ordinary messages", () => {
    expect(parseComponentStyleMessage("hello")).toBeNull()
  })

  it("splits instruction from style-edit context", () => {
    const merged = mergeComponentStyleWithDraft("Apply please", [sample()])
    const parsed = parseComponentStyleMessage(merged)
    expect(parsed).not.toBeNull()
    expect(parsed?.instruction).toBe("Apply please")
    expect(parsed?.editCount).toBe(1)
  })
})

import { describe, expect, it } from "vitest"
import { inferArtifactKind } from "./types"

describe("inferArtifactKind", () => {
  describe("csv / tsv", () => {
    it("detects .csv", () => {
      expect(inferArtifactKind("output.csv")).toBe("csv")
    })
    it("detects .tsv", () => {
      expect(inferArtifactKind("data.tsv")).toBe("csv")
    })
    it("is case-insensitive", () => {
      expect(inferArtifactKind("REPORT.CSV")).toBe("csv")
    })
  })

  describe("spreadsheet", () => {
    it("detects .xlsx", () => {
      expect(inferArtifactKind("report.xlsx")).toBe("spreadsheet")
    })
    it("detects .xls", () => {
      expect(inferArtifactKind("old.xls")).toBe("spreadsheet")
    })
    it("detects .ods", () => {
      expect(inferArtifactKind("sheet.ods")).toBe("spreadsheet")
    })
  })

  describe("presentation", () => {
    it("detects .pptx", () => {
      expect(inferArtifactKind("deck.pptx")).toBe("presentation")
    })
    it("detects .ppt", () => {
      expect(inferArtifactKind("old.ppt")).toBe("presentation")
    })
    it("detects .key", () => {
      expect(inferArtifactKind("keynote.key")).toBe("presentation")
    })
  })

  describe("image", () => {
    it("detects .png", () => {
      expect(inferArtifactKind("banner.png")).toBe("image")
    })
    it("detects .jpg", () => {
      expect(inferArtifactKind("photo.jpg")).toBe("image")
    })
    it("detects .jpeg", () => {
      expect(inferArtifactKind("photo.jpeg")).toBe("image")
    })
    it("detects .webp", () => {
      expect(inferArtifactKind("icon.webp")).toBe("image")
    })
    it("detects .gif", () => {
      expect(inferArtifactKind("anim.gif")).toBe("image")
    })
  })

  describe("diagram", () => {
    it("detects .svg", () => {
      expect(inferArtifactKind("flow.svg")).toBe("diagram")
    })
    it("detects .mmd (mermaid)", () => {
      expect(inferArtifactKind("diagram.mmd")).toBe("diagram")
    })
    it("detects .dot (graphviz)", () => {
      expect(inferArtifactKind("graph.dot")).toBe("diagram")
    })
  })

  describe("document", () => {
    it("detects .pdf", () => {
      expect(inferArtifactKind("spec.pdf")).toBe("document")
    })
    it("detects .docx", () => {
      expect(inferArtifactKind("notes.docx")).toBe("document")
    })
  })

  describe("code files return null", () => {
    it.each([
      "main.ts",
      "App.tsx",
      "lib.rs",
      "mod.py",
      "index.js",
      "component.jsx",
      "main.go",
      "App.java",
      "config.toml",
      "yarn.lock",
      "build.gradle",
      "Makefile",
      "Dockerfile",
      ".eslintrc",
    ])("returns null for %s", (path) => {
      expect(inferArtifactKind(path)).toBeNull()
    })
  })

  describe("artifact directories promote generic files", () => {
    it("promotes .md in reports/", () => {
      expect(inferArtifactKind("reports/summary.md")).toBe("document")
    })
    it("promotes .json in exports/", () => {
      expect(inferArtifactKind("exports/data.json")).toBe("document")
    })
    it("promotes .yaml in artifacts/", () => {
      expect(inferArtifactKind("artifacts/spec.yaml")).toBe("document")
    })
    it("promotes .txt in plans/", () => {
      expect(inferArtifactKind("plans/roadmap.txt")).toBe("document")
    })
    it("promotes nested path under /reports/", () => {
      expect(inferArtifactKind("project/reports/q1.html")).toBe("document")
    })
    it("does NOT promote .ts in artifacts/ (still code)", () => {
      expect(inferArtifactKind("artifacts/helper.ts")).toBeNull()
    })
    it("does NOT promote .rs in reports/", () => {
      expect(inferArtifactKind("reports/parser.rs")).toBeNull()
    })
  })

  describe("edge cases", () => {
    it("returns null for empty string", () => {
      expect(inferArtifactKind("")).toBeNull()
    })
    it("returns null for no extension", () => {
      expect(inferArtifactKind("README")).toBeNull()
    })
    it("handles nested paths correctly", () => {
      expect(inferArtifactKind("src/assets/logo.png")).toBe("image")
      expect(inferArtifactKind("src/main.py")).toBeNull()
    })
    it("handles Windows-style paths", () => {
      expect(inferArtifactKind("C:\\Users\\me\\report.pdf")).toBe("document")
    })
  })
})

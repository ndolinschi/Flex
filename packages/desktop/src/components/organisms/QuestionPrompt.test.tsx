import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { QuestionPrompt } from "./QuestionPrompt"
import type { PendingQuestion } from "../../lib/types"

/**
 * Layout regressions for the AskUserQuestion wizard: mid-step single-select
 * used to always paint an empty footer (invisible Back + border + padding),
 * which hollowed out the bottom of the card on a wide content rail.
 */

const baseQuestion = (
  overrides: Partial<PendingQuestion> = {},
): PendingQuestion => ({
  sessionId: "s-1",
  requestId: "q-1",
  questions: [
    {
      header: "Project name",
      question: "What should the project be named (currently 'temp-app')?",
      options: [
        { label: "Keep temp-app" },
        { label: "restaurant-menus" },
      ],
      multi_select: false,
      allow_custom: true,
    },
    {
      header: "Stack",
      question: "Which stack?",
      options: [{ label: "Next.js" }],
      multi_select: false,
      allow_custom: false,
    },
  ],
  ...overrides,
})

describe("QuestionPrompt layout", () => {
  it("omits the hollow footer on the first single-select step", () => {
    const html = renderToStaticMarkup(
      <QuestionPrompt question={baseQuestion()} />,
    )

    expect(html).toContain("Agent needs your input")
    expect(html).toContain("1 of 2")
    expect(html).toContain("Project name")
    expect(html).toContain("Keep temp-app")
    // No Back / Next chrome when options auto-advance.
    expect(html).not.toContain(">Back<")
    expect(html).not.toContain(">Next<")
    expect(html).not.toContain(">Submit<")
  })

  it("shows Submit on a single-question last step", () => {
    const html = renderToStaticMarkup(
      <QuestionPrompt
        question={baseQuestion({
          questions: [
            {
              header: "Name",
              question: "Name?",
              options: [{ label: "A" }],
              multi_select: false,
              allow_custom: true,
            },
          ],
        })}
      />,
    )

    expect(html).toContain(">Submit<")
    expect(html).not.toContain("1 of")
  })

  it("uses a stable padding/spacing scale on the docked card", () => {
    const html = renderToStaticMarkup(
      <QuestionPrompt question={baseQuestion()} />,
    )
    expect(html).toContain("data-question-prompt")
    expect(html).toContain("px-3.5")
    expect(html).toContain("pt-3")
    expect(html).toContain("pb-3")
    expect(html).toContain("gap-2")
    expect(html).toContain("mt-2.5")
  })
})

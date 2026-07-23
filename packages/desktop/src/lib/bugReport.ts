export const BUG_REPORT_ISSUES_URL =
  "https://github.com/ndolinschi/Flex/issues/new"

export const BUG_REPORT_TERMS_URL =
  "https://github.com/ndolinschi/Flex/blob/main/LICENSE-MIT"
export const BUG_REPORT_PRIVACY_URL =
  "https://docs.github.com/en/site-policy/privacy-policies/github-privacy-statement"

export type BugReportContext = {
  appVersion: string
  os: string
  sessionId: string | null
  taskIds: string[]
}

export const buildBugReportUrl = (
  description: string,
  ctx: BugReportContext,
): string => {
  const trimmed = description.trim()
  const tasks =
    ctx.taskIds.length > 0
      ? ctx.taskIds.map((id) => `- \`${id}\``).join("\n")
      : "- _(none recorded this session)_"
  const body = [
    "## What went wrong",
    "",
    trimmed || "_(no description)_",
    "",
    "## Diagnostics (auto-included)",
    "",
    `- App version: \`${ctx.appVersion || "unknown"}\``,
    `- OS: \`${ctx.os || "unknown"}\``,
    `- Active session: \`${ctx.sessionId ?? "none"}\``,
    `- Task / session IDs:`,
    tasks,
    "",
    "---",
    "_Submitted via in-app Submit Bug. Do not include secrets or personal data._",
  ].join("\n")

  const title =
    trimmed.length > 0
      ? trimmed.length > 72
        ? `${trimmed.slice(0, 69)}…`
        : trimmed
      : "Bug report"

  const url = new URL(BUG_REPORT_ISSUES_URL)
  url.searchParams.set("title", title)
  url.searchParams.set("body", body)
  return url.toString()
}

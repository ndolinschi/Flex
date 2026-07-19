import { describe, expect, it } from "vitest"
import type { TimelineRow, ToolCall } from "./types"
import {
  clusterWorkRows,
  collectRunningWorkers,
  runningWorkersSignature,
  stripMatchedAgentToolRows,
  summarizeWorkerActivity,
  workersHeaderLabel,
} from "./workerPresentation"
import { SUBAGENT_TOOL_NAME } from "./timeline/parseWorkflow"

let callSeq = 0

const makeCall = (
  overrides: Partial<ToolCall> & { tool_name: string },
): ToolCall => {
  callSeq += 1
  return {
    id: `call-${callSeq}`,
    session_id: "s-1",
    turn_id: "turn-1",
    message_id: "m-1",
    input: {},
    read_only: false,
    origin: { origin: "model" },
    status: { state: "completed" },
    timing: { queued_at_ms: 0 },
    result: { content: [{ type: "markdown", text: "" }], is_error: false },
    ...overrides,
  }
}

const subagent = (
  overrides: Partial<Extract<TimelineRow, { type: "subagent" }>> & {
    childSession: string
    task: string
  },
): Extract<TimelineRow, { type: "subagent" }> => ({
  type: "subagent",
  id: `sub:${overrides.childSession}`,
  role: "worker",
  phase: "started",
  children: [],
  tsMs: 1,
  ...overrides,
})

describe("stripMatchedAgentToolRows", () => {
  it("drops Agent tool rows whose id matches a subagent callId", () => {
    const a1 = makeCall({ tool_name: SUBAGENT_TOOL_NAME, status: { state: "running" } })
    const a2 = makeCall({ tool_name: SUBAGENT_TOOL_NAME, status: { state: "running" } })
    const read = makeCall({ tool_name: "Read", input: { path: "a.ts" } })
    const rows = [
      { type: "tool", id: a1.id, call: a1 },
      { type: "tool", id: a2.id, call: a2 },
      { type: "tool", id: read.id, call: read },
      subagent({ childSession: "c1", task: "UI", callId: a1.id }),
      subagent({ childSession: "c2", task: "API", callId: a2.id }),
    ]
    const out = stripMatchedAgentToolRows(rows)
    expect(out.map((r) => r.type)).toEqual([
      "tool",
      "subagent",
      "subagent",
    ])
    expect(out[0]).toMatchObject({ type: "tool", call: { id: read.id } })
  })
})

describe("clusterWorkRows", () => {
  it("groups consecutive subagents and strips matching Agent tools", () => {
    const a1 = makeCall({ tool_name: SUBAGENT_TOOL_NAME })
    const a2 = makeCall({ tool_name: SUBAGENT_TOOL_NAME })
    const clusters = clusterWorkRows([
      { type: "tool", id: a1.id, call: a1 },
      { type: "tool", id: a2.id, call: a2 },
      subagent({ childSession: "c1", task: "UI", callId: a1.id, phase: "completed" }),
      subagent({ childSession: "c2", task: "API", callId: a2.id, phase: "started" }),
    ])
    expect(clusters).toHaveLength(1)
    expect(clusters[0].kind).toBe("workers")
    if (clusters[0].kind === "workers") {
      expect(clusters[0].workers).toHaveLength(2)
    }
  })

  it("keeps tool clusters on either side of a workers group", () => {
    const read = makeCall({ tool_name: "Read", input: { path: "a.ts" } })
    const clusters = clusterWorkRows([
      { type: "tool", id: read.id, call: read },
      subagent({ childSession: "c1", task: "UI" }),
      subagent({ childSession: "c2", task: "API" }),
      { type: "assistant", id: "asst-1", text: "Done" },
    ])
    expect(clusters.map((c) => c.kind)).toEqual([
      "tools",
      "workers",
      "other",
    ])
  })
})

describe("summarizeWorkerActivity", () => {
  it("reports running tool label and tool count", () => {
    const call = makeCall({
      tool_name: "Read",
      input: { path: "Foo.tsx" },
      status: { state: "running" },
    })
    const activity = summarizeWorkerActivity(
      [{ type: "tool", id: "t1", call, tsMs: 1 }],
      "started",
    )
    expect(activity.status).toBe("running")
    expect(activity.toolCount).toBe(1)
    expect(activity.latestLabel).toMatch(/Foo|Read/)
  })
})

describe("workersHeaderLabel", () => {
  it("labels running and settled groups", () => {
    expect(
      workersHeaderLabel([
        { phase: "started" },
        { phase: "started" },
      ]),
    ).toBe("Working with 2 agents")
    expect(
      workersHeaderLabel([
        { phase: "completed" },
        { phase: "completed" },
      ]),
    ).toBe("Worked with 2 agents")
    expect(
      workersHeaderLabel([
        { phase: "started" },
        { phase: "completed" },
      ]),
    ).toBe("Working with 1 of 2 agents")
  })
})

describe("collectRunningWorkers", () => {
  it("finds started subagents and workflow slots", () => {
    const rows: TimelineRow[] = [
      subagent({ childSession: "c1", task: "A", phase: "started" }),
      subagent({ childSession: "c2", task: "B", phase: "completed" }),
      {
        type: "workflow",
        id: "wf-1",
        callId: "call-wf",
        toolName: "RunWorkflow",
        steps: [],
        status: { state: "running" },
        subagents: [
          {
            childSession: "c3",
            task: "C",
            phase: "started",
            children: [],
          },
        ],
        tsMs: 1,
      },
    ]
    const running = collectRunningWorkers(rows)
    expect(running.map((w) => w.childSession).sort()).toEqual(["c1", "c3"])
  })
})

describe("runningWorkersSignature", () => {
  it("stays stable across unrelated assistant markdown growth", () => {
    const base: TimelineRow[] = [
      subagent({ childSession: "c1", task: "A", phase: "started" }),
      {
        type: "assistant",
        id: "a1",
        messageId: "m1",
        text: "hello",
        tsMs: 1,
      },
    ]
    const grown: TimelineRow[] = [
      subagent({ childSession: "c1", task: "A", phase: "started" }),
      {
        type: "assistant",
        id: "a1",
        messageId: "m1",
        text: "hello world from a long streaming delta",
        tsMs: 1,
      },
    ]
    expect(runningWorkersSignature(base)).toBe(runningWorkersSignature(grown))
  })

  it("changes when a nested tool status flips", () => {
    const running = makeCall({
      tool_name: "Read",
      status: { state: "running" },
    })
    const completed = { ...running, status: { state: "completed" as const } }
    const before: TimelineRow[] = [
      subagent({
        childSession: "c1",
        task: "A",
        phase: "started",
        children: [{ type: "tool", id: running.id, call: running, tsMs: 1 }],
      }),
    ]
    const after: TimelineRow[] = [
      subagent({
        childSession: "c1",
        task: "A",
        phase: "started",
        children: [{ type: "tool", id: completed.id, call: completed, tsMs: 1 }],
      }),
    ]
    expect(runningWorkersSignature(before)).not.toBe(
      runningWorkersSignature(after),
    )
  })
})

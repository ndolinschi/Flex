---
name: record-fixtures
description: How to update golden/insta snapshots and delegator wire-format fixtures in Flex. Use when a parser, mapper, or system-prompt change breaks snapshot tests, or when adding a new external-agent connector.
---

# Record fixtures

Hard rule (AGENTS.md #7): golden tests are **re-recorded, never hand-edited**.

## Insta snapshots (prompts, reducers, parsers)

```bash
cd packages/engine
cargo insta test              # run, collect pending snapshots
cargo insta review            # inspect and accept/reject each diff
# or non-interactive when the change is fully intended:
INSTA_UPDATE=always cargo test -p <crate>
```

Always read the diff before accepting — a surprising snapshot change means the
code change is wrong, not the snapshot.

## Delegator wire-format fixtures

Mapper unit tests in `packages/providers/crates/delegators/*/src/mapper.rs`
embed raw JSON lines captured from the **real CLI** as `r#"..."#` strings.
To (re)record:

1. Run the CLI directly and capture stdout, e.g.:
   - `claude -p "say hi" --output-format stream-json`
   - `opencode run "say hi" --print-logs` (verify current flags first)
   - `cursor-agent --print --output-format stream-json "say hi"`
2. Paste representative lines (assistant text, tool call, tool result, final
   result/usage) into the test fixtures verbatim.
3. Assert the mapped `Vec<DelegatorEvent>`; unknown frame types must map to
   `DelegatorEvent::Unknown`, never panic.

Never invent wire formats from documentation alone — mark unverified fixtures
with a `// UNVERIFIED: recorded from docs, not a live CLI` comment.

When NOT to use: a snapshot diff you haven't read yet (read it first — a
surprising diff usually means the code change is wrong, not the snapshot);
recording a fixture for a wire format you haven't captured from a real CLI run
(see the "never invent" rule above); routine test failures unrelated to a
parser/mapper/system-prompt change (that's a bug to fix, not a fixture to
re-record).

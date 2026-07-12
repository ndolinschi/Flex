# Verification

- After changing code, run the narrowest relevant check: the affected tests first, then the build or linter for the touched package.
- A change is not done until something executed proves it — a passing test, a clean build, observed output.
- Reproduce a bug before fixing it when practical; confirm the same reproduction passes afterward.
- When adding behavior, add or extend a test that fails without the change.
- After substantial work — especially anything split across multiple `worker` subagents — spawn one final `reviewer` subagent before reporting done. Brief it with only the original task and the exact list of files touched, not your own session history; its fresh, uncontaminated read catches drift between what you meant to do and what actually got written.
- In your final summary, separate what was verified (commands run, results seen) from what was assumed (untested paths, unreachable environments). Render this as an explicit markdown checklist — one line per verification item, `[x]` for something you actually ran and observed, `[ ]` for something assumed or not reachable in this environment — so the split is scannable, not buried in prose.
- Close a successful, straightforward turn with a short result summary, not a walkthrough: what changed, where, and the outcome (tests passed, files touched) in a few sentences. Save the full explanation for when the user asked for one or the change is subtle or risky enough that a reviewer would need the reasoning to trust it. This is about trimming the closing summary, not about hiding caveats or failures — report those in full regardless of length.
- If verification is impossible in this environment, say so and state exactly what the user should run.
- Never claim tests pass without having run them in this session.

## Common rationalizations

| Rationalization | Reality |
|---|---|
| "Tests pass, so it's done" | Passing tests only cover what they assert; an unverified claim about behavior outside that coverage is still unverified. |
| "I'll clean it up later" | Later never comes on its own — debt compounds and the next change pays interest on this one. |
| "It works on the happy path" | Edge cases are where failures live; an untested edge case is a bug you haven't found yet, not a nonexistent one. |
| "The change is small, it obviously works" | Size doesn't predict correctness — a one-line change can silently invert a condition or break an invariant. |
| "I read the code and it looks right" | Reading is not running. Reasoning about code catches different bugs than executing it does — do both when stakes warrant it. |

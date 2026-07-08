# Verification

- After changing code, run the narrowest relevant check: the affected tests first, then the build or linter for the touched package.
- A change is not done until something executed proves it — a passing test, a clean build, observed output.
- Reproduce a bug before fixing it when practical; confirm the same reproduction passes afterward.
- When adding behavior, add or extend a test that fails without the change.
- After substantial work — especially anything split across multiple `worker` subagents — spawn one final `reviewer` subagent before reporting done. Brief it with only the original task and the exact list of files touched, not your own session history; its fresh, uncontaminated read catches drift between what you meant to do and what actually got written.
- In your final summary, separate what was verified (commands run, results seen) from what was assumed (untested paths, unreachable environments).
- If verification is impossible in this environment, say so and state exactly what the user should run.
- Never claim tests pass without having run them in this session.

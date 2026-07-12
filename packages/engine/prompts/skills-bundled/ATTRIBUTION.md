# Attribution

The `SKILL.md` files in this directory (except `using-flex-skills/SKILL.md`, which is
original) are vendored from:

- **Source**: [addyosmani/agent-skills](https://github.com/addyosmani/agent-skills)
- **License**: MIT (Copyright (c) 2025 Addy Osmani) — see the upstream `LICENSE` file
  for full text. Reproduced in full below.

## Vendored skills

| Local directory | Upstream path |
|---|---|
| `debugging-and-error-recovery/` | `skills/debugging-and-error-recovery/SKILL.md` |
| `code-simplification/` | `skills/code-simplification/SKILL.md` |
| `performance-optimization/` | `skills/performance-optimization/SKILL.md` |
| `security-and-hardening/` | `skills/security-and-hardening/SKILL.md` |
| `doubt-driven-development/` | `skills/doubt-driven-development/SKILL.md` |

## Local modifications

All five files were swept for brand references and harness-specific instructions that
would mislead this engine's models, then adapted as follows:

- **`code-simplification/SKILL.md`**: replaced the "Inspired by the Claude Code
  Simplifier plugin" attribution line with a generic community-skill credit; replaced
  two `CLAUDE.md`-specific references (Step 2 of "Follow Project Conventions", and the
  Verification checklist) with `AGENTS.md (or your agent harness's project-conventions
  doc)` — this repo's actual conventions file, per `AGENTS.md`'s own note that
  `CLAUDE.md` is only a thin pointer here.
- **`doubt-driven-development/SKILL.md`**: substantially rewritten wherever it assumed
  the upstream host's persona/subagent roster:
  - "Loading Constraints" no longer talks about persona `skills:` frontmatter or a
    Claude-Code-specific "nested subagent spawn" restriction; it now names this
    engine's actual mechanism — `RoleSpec.max_depth` and `RoleRegistry::tool_filter`,
    which strip `Agent`/`Verify`/`RunWorkflow` once a role's spawn-depth budget is
    exhausted — as the reason self-review from inside a subagent competes with that
    subagent's own spawn budget.
  - Step 3 (DOUBT) no longer points at an `agents/` directory of role-based personas;
    it names the concrete mechanism instead — spawn the built-in `reviewer` role via
    the `Agent` tool for an in-flight adversarial read, or the independent `Verify`
    tool (`verifier` role, `agentloop-verifier` plugin, "maker is never the grader")
    for a finished artifact.
  - Every `/review` reference (a slash command this engine does not have) was replaced
    with `Verify`, the tool that plays the equivalent post-hoc-verdict role here.
  - "Interaction with Other Skills" dropped the reference to a `code-review-and-quality`
    skill and a `references/orchestration-patterns.md` file that don't exist in this
    bundle, replacing them with the `Verify` tool and this engine's actual depth-budget
    mechanism.
  - No other content — the CLAIM/EXTRACT/DOUBT/RECONCILE/STOP cycle, the cross-model
    escalation protocol, the rationalizations table, and the verification checklist —
    was changed beyond these mechanism swaps.
- **`debugging-and-error-recovery/SKILL.md`**, **`performance-optimization/SKILL.md`**,
  **`security-and-hardening/SKILL.md`**: copied verbatim. They contained no brand
  references or harness-specific instructions to neutralize.

No "Cursor" references were found in any vendored file.

## Upstream license (MIT)

```
MIT License

Copyright (c) 2025 Addy Osmani

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

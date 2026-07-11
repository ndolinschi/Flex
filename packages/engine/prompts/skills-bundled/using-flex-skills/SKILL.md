---
name: using-flex-skills
description: Decision tree mapping task types to this engine's bundled skills and built-in roles/tools. Use at the start of a task, or whenever you're unsure whether a skill or role would help before you dive in. This is the meta-skill governing how the other bundled skills get picked.
---

# Using Flex Skills

## Overview

This engine ships five bundled skills plus a handful of built-in roles and tools that
overlap with them. Skills encode *how* to work a problem; roles and tools are *what you
spawn or call* to do it. This meta-skill is the lookup table between "what kind of task
is this" and "which skill/role/tool applies" — check it before starting non-trivial
work, not after you're stuck.

## Decision Tree

```
Task arrives
    │
    ├── Something broke (test fails, build breaks, wrong behavior)?
    │       → debugging-and-error-recovery
    │
    ├── Code works but is harder to read/maintain/extend than it should be?
    │       → code-simplification
    │
    ├── Web/service is slow, or you're about to guess at a performance fix?
    │       → performance-optimization
    │
    ├── About to merge, ship, or touch anything security-sensitive
    │   (auth, input handling, secrets, deserialization, external input)?
    │       → security-and-hardening
    │
    ├── About to make a non-trivial decision (branching logic, module
    │   boundary, an invariant the compiler can't check, unfamiliar code,
    │   irreversible blast radius)?
    │       → doubt-driven-development (spawn a `reviewer` subagent via `Agent`
    │         for an in-flight adversarial read)
    │
    └── Wrapping up substantial work, about to report done?
            → spawn a `reviewer` subagent (fresh, uncontaminated context) before
              reporting; for a stricter finished-artifact verdict against a
              rubric, use the `Verify` tool (`verifier` role) instead — see
              "Skills vs. built-in roles and tools" below.
```

More than one branch can apply to the same task — e.g. fixing a bug
(`debugging-and-error-recovery`) may leave the fix in a state worth simplifying
(`code-simplification`) before you call `Verify`. Work them in sequence, not in
parallel confusion.

## Skills vs. Built-in Roles and Tools

Skills are markdown process guides you load into context; roles and tools are how you
actually act on that guidance. Know which of these you're reaching for:

| Need | Reach for |
|---|---|
| A read-only fresh-context reviewer for an in-flight decision | `Agent` tool, `reviewer` role |
| An independent verdict on a finished artifact against a rubric ("maker is never the grader") | `Verify` tool, `verifier` role — cannot see how the work was produced, only the artifacts and rubric |
| A full implementation subagent for a self-contained subtask | `Agent` tool, `worker` role (full tool access) |
| A read-only research subagent | `Agent` tool, `searcher` role |
| A declarative multi-step pipeline whose shape you already know | `RunWorkflow` tool (if `enable_workflow_tool` is on) |
| A process guide for *how* to approach the work | one of the five skills above |

## When Skills Overlap With Verification

`30-verification.md`'s standing rule — spawn one final `reviewer` subagent after
substantial or multi-worker work, briefed with only the original task and the touched
files — is not optional and is not replaced by any of these skills. Doubt-driven
development's adversarial review is a stricter, earlier version of the same idea
applied per-decision instead of once at the end; running both is normal for
high-stakes work, not redundant.

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "I already know which skill to use, I'll skip the lookup" | Fine once you're confident — but if you're reaching for `Bash` to grep the skills directory first, you already need this table more than you think. |
| "This is a small task, no skill needed" | Small and low-stakes tasks genuinely don't need one — but check the "when NOT to use" section of the skill you're tempted to skip before deciding that for yourself. |
| "I'll just spawn a generic reviewer, roles are overkill" | The built-in `reviewer`/`verifier` roles already carry the right tool restrictions and prompts — reinventing that ad hoc in a brief risks giving a review subagent tools (Bash, Write, Edit) it shouldn't have. |

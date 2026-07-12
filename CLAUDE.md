# CLAUDE.md

Read **AGENTS.md** — the single source of truth for architecture, layer contracts,
dependency rules, hard rules, and commit style. Do not add guidance here.

When looking for information about the solution, always start with **ARCHITECTURE.md**,
**TECHSTACK.md**, and **COMPONENTS.md** (system map, tooling, per-crate catalog) before
grepping; AGENTS.md holds the rules.

Most-used commands:

```bash
# full verify (run in every package a change touches)
for pkg in engine providers search sdk gateway index; do
  ( cd packages/$pkg && cargo fmt --all --check \
    && cargo clippy --workspace --all-targets --all-features -- -D warnings \
    && cargo test --workspace --all-features )
done
( cd packages/engine && cargo xtask schema --check )   # schema drift gate
( cd packages/engine && cargo xtask schema )           # regenerate schemas
```

# CLAUDE.md

Read **AGENTS.md** — the single source of truth for architecture, layer contracts,
dependency rules, hard rules, and commit style. Do not add guidance here.

Most-used commands:

```bash
# full verify (run in every package a change touches)
for pkg in engine providers search sdk; do
  ( cd packages/$pkg && cargo fmt --all --check \
    && cargo clippy --workspace --all-targets --all-features -- -D warnings \
    && cargo test --workspace --all-features )
done
( cd packages/engine && cargo xtask schema --check )   # schema drift gate
( cd packages/engine && cargo xtask schema )           # regenerate schemas
```

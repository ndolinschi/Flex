---
name: verify
description: Run the full pre-commit verify loop across the four Flex cargo workspaces (fmt, clippy -D warnings, tests, schema drift gate). Use before every commit and after any cross-package change.
---

# Verify

Each package is its own cargo workspace. Run the full loop in **every package the
change touches**; a change to `packages/engine` must also be verified in
`providers`, `search`, and `sdk` (they path-depend on engine crates).

```bash
for pkg in engine providers search sdk; do
  ( cd packages/$pkg
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace --all-features )
done
# engine only: schema drift gate
( cd packages/engine && cargo xtask schema --check )
```

Rules:
- Clippy warnings are errors — fix them, never `#[allow]` without justification.
- If `schema --check` fails after a contracts change, run `cargo xtask schema`
  and commit the regenerated `packages/engine/schemas/v1/*.json`.
- Brand gate: the string "flex" may appear only in
  `packages/sdk/crates/sdk/Cargo.toml` (CI greps for leaks).
- If an insta snapshot test fails, review the diff first; accept only intended
  changes (see the record-fixtures skill).

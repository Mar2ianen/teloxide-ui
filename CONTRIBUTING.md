# Contributing

Thanks for helping build `teloxide-ui`.

This project is intentionally architecture-first while the runtime is young.
Read `AGENTS.md`, `CODE_STYLE.md`, and `docs/ARCHITECTURE.md` before implementing a new subsystem.

## Before opening a change

For bug fixes, include a minimal reproduction where practical.

For new runtime behavior, describe:

- the state/action semantics;
- the surface affected;
- ordering and concurrency behavior;
- failure behavior;
- whether the change belongs here or in teloxide;
- how stale input is handled.

## Local checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo test --doc
cargo doc --no-deps --all-features
```

## Branches

Use short descriptive branches, for example:

```text
feat/action-registry
feat/rich-renderer
fix/stale-callback-race
docs/surface-contract
```

## Pull requests

Keep PRs narrow enough that the invariant being changed is reviewable.

A good PR description contains:

1. Problem.
2. Proposed model.
3. Alternatives considered.
4. Concurrency/failure semantics.
5. Public API impact.
6. Tests.

If a change requires a generic teloxide primitive, prefer a separate teloxide
PR/commit and keep the UI-side adapter small.

## Public APIs

Before stabilization, breaking changes are allowed but should be deliberate.

Do not preserve a bad abstraction merely for compatibility during the
experimental phase. Conversely, avoid gratuitous churn after an invariant has
already been proven.

## Documentation

Behavioral changes and documentation changes belong in the same PR.

# AGENTS.md

Instructions for coding agents and automated contributors working in this
repository.

These rules are part of the project architecture. Do not silently weaken them
to make an implementation easier.

## Project identity

`teloxide-ui` is a **standalone crate that depends on teloxide**.

It is not a module waiting to be copied into teloxide. Shared lower-level work
may be proposed for teloxide when it is genuinely useful without this UI
runtime; application/UI concepts stay here.

The initial teloxide target is `Mar2ianen/teloxide-fork`, because that fork
contains the Bot API 10.3 Rich Message surface and reusable outbound/Drafter
runtime work required by this project.

## Read first

Before changing architecture, read:

1. `README.md`
2. `docs/ARCHITECTURE.md`
3. `docs/TELOXIDE_BOUNDARY.md`
4. `docs/ROADMAP.md`
5. `CODE_STYLE.md`

If code and docs disagree, do not guess. Treat the invariant sections in
`docs/ARCHITECTURE.md` as the intended contract and update code/docs together.

## Hard architecture invariants

### 1. Server owns authoritative state

Do not place authoritative application state in Telegram messages,
`callback_data`, rendered button labels, or client-visible text.

Client input is a request to perform an action, not proof of current state.

### 2. `view()` is pure

The view layer is a deterministic projection:

```text
State → Ui<Action>
```

Do not perform Telegram requests, persistence, filesystem I/O, clocks, random
number generation, or hidden mutations from rendering code.

If an operation has a side effect, model it as an explicit transition/effect.

### 3. Do not invent a DOM

Telegram does not provide arbitrary Rich Message subtree patching.

Components may form a server-side tree for composition, but the public model
must not promise browser-like node identity, CSS, layout measurement, hooks, or
DOM patch semantics.

### 4. Surface is the projection ordering boundary

Two renders targeting the same surface must never be able to leave an older
revision visible after a newer revision.

Use teloxide's generic outbound ordering/coalescing infrastructure where
possible. Do not add a second independent rate limiter or scheduler inside this
crate.

### 5. State ordering and network ordering are separate

Never keep a state-store lock, transaction guard, or per-view mutex across a
Telegram network request.

Commit the authoritative state transition first, then project the resulting
desired representation.

### 6. Callback ACK is not render completion

Telegram callback acknowledgement must not depend on a slow render path.

The runtime must make it difficult to forget callback acknowledgement while
still permitting explicit toast/alert responses.

### 7. Callback data is untrusted

Validate:

- token existence;
- token expiry;
- actor/user binding where configured;
- target view/surface;
- revision policy;
- decoded action.

Never assume callback payloads are valid merely because Telegram delivered
them.

### 8. Stale actions are explicit

Every stateful action flow must have an explicit stale-revision policy.

Do not silently apply an old action to unrelated newer state.

### 9. Ephemeral UI is projection

Ephemeral delivery is useful presentation, not durable domain truth.

A domain mutation must remain correct if an ephemeral send/edit is delayed,
missed, or retried.

### 10. Drafter remains Drafter

Do not reimplement streaming preview scheduling, native draft lifecycle, Stop
handling, or delivery lifecycle already provided by teloxide's Drafter.

UI integration should be an adapter/effect over Drafter.

## Teloxide boundary rule

A change belongs in the teloxide fork only when all of the following hold:

- it is useful without importing or understanding `teloxide-ui`;
- its API can be described entirely in Telegram/request/runtime terms;
- it does not contain UI state, component, action-registry, or renderer policy;
- placing it in teloxide avoids duplicated generic transport/runtime logic.

When uncertain, implement the concept locally first behind a narrow abstraction.
Move it downward only after the generic boundary is obvious.

See `docs/TELOXIDE_BOUNDARY.md`.

## Public API style

Prefer:

- typed IDs/newtypes over raw strings where identity matters;
- enums over boolean combinations with illegal states;
- immutable builders for semantic UI construction;
- explicit policy types for stale actions, render/coalescing behavior, and
  actor binding;
- capability traits at integration boundaries;
- owned data at async task boundaries;
- errors that distinguish invalid input, stale state, storage conflict,
  admission failure, and Telegram failure.

Avoid:

- stringly typed action routing;
- global mutable registries hidden behind convenience functions;
- macros that encode architecture before the builder API is stable;
- blanket `Arc<Mutex<_>>` as a substitute for a state model;
- unbounded queues;
- retry loops that ignore delivery certainty;
- swallowing `message is not modified` without proving the operation is a
  representation no-op.

## Callback token design

Opaque callback tokens should be the default for stateful UI.

The server-side record should be able to bind:

- action;
- view id;
- revision;
- actor policy;
- expiry.

Do not serialize arbitrary application objects into callback payloads merely to
avoid a lookup.

Keep the token format versioned from day one.

## Renderer rules

The application produces a complete desired `Ui` representation.

Renderer diffing is an optimization only.

A renderer may:

- skip an identical render;
- use a cheaper Telegram edit method when semantics are identical;
- use Rich Message or legacy markup according to capability/policy.

A renderer must not change application semantics.

## Scheduler integration

Use the shared teloxide outbound queue for Telegram request admission,
rate-limiting, priorities, ordering, and latest-wins pending projection.

Expected priority classes:

- callback acknowledgement: interactive/critical;
- user-triggered projection: interactive;
- ordinary projection: normal;
- background refresh: background.

Exact mappings are implementation details and require tests.

## Persistence model

Persistent stores should support optimistic revision checks or an equivalent
compare-and-set transaction.

The runtime should be able to distinguish:

```text
desired_revision
rendered_revision
```

A failed projection must not roll back an already committed domain transition.

Durable projection may later use teloxide's generic outbox, but at-least-once
delivery must be modeled explicitly.

## Tests required for runtime/concurrency changes

At minimum test:

- stale callback rejection;
- actor-bound token rejection;
- token expiry;
- revision compare-and-set conflict;
- two concurrent actions on the same view;
- two concurrent renders on the same surface;
- pending render coalescing;
- cancellation before scheduler admission;
- Telegram edit failure after state commit;
- ephemeral addressing;
- callback ACK independent from render latency.

Use paused Tokio time where timing policy is involved.

Prefer model/property tests for revision and ordering invariants.

## Documentation rules

Any public behavioral change must update the relevant docs in the same PR.

Architecture changes must update `docs/ARCHITECTURE.md`.

Changes to the teloxide boundary must update `docs/TELOXIDE_BOUNDARY.md`.

Roadmap completion should be reflected in `docs/ROADMAP.md` and
`CHANGELOG.md`.

Do not document speculative APIs as implemented. Mark design sketches as such.

## Dependency policy

Keep dependencies small and justified.

Before adding a crate, state which project-level responsibility it owns and why
the standard library / existing teloxide dependency is insufficient.

Do not add another Telegram Bot API client.

Do not add another general-purpose async runtime.

Do not add a second scheduler/rate limiter.

## Formatting and checks

Before considering a change complete:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo test --doc
cargo doc --no-deps --all-features
```

If a check cannot run in the current environment, report that explicitly.

## Commit/PR discipline

Keep architectural refactors separate from behavior changes when practical.

PR descriptions should state:

- problem;
- chosen invariant/model;
- alternatives rejected;
- compatibility impact;
- concurrency/failure semantics;
- tests.

Do not hide breaking public API changes in refactors.

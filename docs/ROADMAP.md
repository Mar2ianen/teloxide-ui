# Roadmap

The roadmap is ordered by architectural risk, not by feature count.

## Phase 0 — repository contract

- [x] standalone crate boundary
- [x] MIT license
- [x] architecture document
- [x] teloxide boundary document
- [x] agent/contributor rules
- [x] CI skeleton
- [x] initial semantic type skeleton

Exit condition: contributors can explain where state, actions, rendering,
scheduling, and Drafter each live.

## Phase 1 — minimal state-driven Rich UI

Goal: prove one message can behave as a deterministic server-driven application
surface.

- [x] semantic `Ui<Action>` AST (text, paragraphs, headings, button rows,
  fragments)
- [x] Rich Message compiler
- [x] baseline structural validation before network requests
- [x] `Message` surface
- [x] `Ephemeral` surface
- [x] inline surface addressing
- [x] view IDs
- [x] monotonic revisions
- [x] opaque action token format
- [x] in-memory action registry
- [x] actor binding
- [x] TTL/expiry
- [x] stale revision rejection
- [x] in-memory `UiStore` with compare-and-set
- [ ] callback dispatcher integration
- [ ] automatic callback ACK
- [x] per-surface outbound serial lane
- [x] latest-wins pending projection admission
- [ ] counter example
- [ ] chess reference example

Phase 1 exit condition: rapid concurrent chess clicks cannot produce an older
visible board after a newer committed state. The current worker proves the
surface queue boundary, but the callback dispatcher and reference application
needed for this end-to-end exit condition are not implemented yet.

## Phase 2 — persistence and multi-surface effects

- [ ] store trait stabilization
- [ ] SQLite store
- [ ] Redis/Postgres evaluation
- [ ] desired vs rendered revisions
- [ ] projection recovery after restart
- [ ] `RenderAt`
- [ ] `Send`
- [ ] `Delete`
- [ ] toast/alert effects
- [ ] ephemeral overlay flow
- [ ] inline-message surface
- [ ] durable projection design using teloxide outbox
- [ ] observability/metrics hooks

Exit condition: a committed state survives process restart and eventually
reprojects without replaying the domain transition.

## Phase 3 — Drafter and long-running operations

- [ ] Drafter effect adapter
- [ ] native rich draft integration
- [ ] edit-in-place streaming integration
- [ ] Stop flow
- [ ] cancellation propagation
- [ ] generation ownership/revision policy
- [ ] progress UI reference example

Exit condition: a long-running generation can stream, stop, and reconcile with
the owning stateful UI without a second scheduler.

## Phase 4 — ergonomics

Only after the builder/runtime contracts are stable:

- [ ] `#[derive(UiAction)]`
- [ ] compact action tag generation
- [ ] optional `ui!` macro
- [ ] reusable component library
- [ ] pagination
- [ ] confirmation component
- [ ] tables
- [ ] form/ForceReply helpers
- [ ] snapshot renderer
- [ ] debug renderer

Exit condition: macros reduce boilerplate without becoming required to
understand or test the runtime.

## Phase 5 — compatibility and release

- [ ] legacy text + `InlineKeyboardMarkup` renderer
- [ ] capability/policy based renderer selection
- [ ] crates.io-compatible teloxide dependency
- [ ] public API review
- [x] MSRV review (1.88 for the current dependency graph)
- [ ] docs.rs
- [ ] migration/versioning policy
- [ ] first publishable release

## Explicitly deferred

These are not v0.x requirements:

- arbitrary browser-like subtree patching;
- CSS;
- client-side state runtime;
- Mini App emulation;
- distributed consensus for arbitrary cross-chat transactions;
- generalized UI language independent of Telegram.

They may be revisited only if real applications demonstrate a need.

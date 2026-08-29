# Code style

This file covers implementation style. Architectural constraints live in
`AGENTS.md` and `docs/ARCHITECTURE.md`.

## Rust

- Format with `rustfmt`.
- Clippy warnings are errors in CI.
- Prefer explicit semantic types over raw strings/integers for identities and
  policies.
- Prefer enums when combinations of booleans would create invalid states.
- Keep public constructors small and unsurprising.
- Use `#[must_use]` for immutable builders and values whose ignored result is
  likely a bug.
- Public items require rustdoc once they are part of the intended API.
- Avoid `unsafe`; the crate root currently forbids it.
- Avoid hidden global state.

## Async and concurrency

- Never hold a store/view lock across a Telegram request.
- Make queue bounds explicit.
- Cancellation behavior is part of the API contract.
- Do not spawn detached tasks without an ownership/shutdown story.
- Prefer deterministic paused-time tests for timing behavior.

## Errors

Errors should preserve the layer that failed.

Do not flatten these into one opaque error:

- invalid action input;
- stale state;
- actor authorization;
- store conflict;
- rendering/validation;
- scheduler admission;
- Telegram transport/API failure.

Add context at boundaries without leaking tokens or private payloads.

## Naming

Use terminology consistently:

- **view**: logical stateful application instance;
- **surface**: Telegram projection target;
- **render**: `State → Ui` and/or `Ui → Telegram representation`, qualified
  where ambiguity matters;
- **projection**: network operation making a surface match desired UI;
- **action**: typed user/application input;
- **effect**: explicit side effect requested by a transition;
- **revision**: monotonic logical version.

Do not call a Telegram message a DOM node or component instance.

## Modules

Keep modules responsibility-oriented rather than type-count-oriented.

Expected long-term split:

```text
action
component
effect
node
render/
runtime/
store/
surface
```

Avoid `utils.rs` for domain concepts.

## Tests

Name tests after invariants/behavior, for example:

```text
stale_action_is_rejected_before_transition
newer_surface_revision_cannot_be_overtaken
callback_ack_does_not_wait_for_projection
```

Prefer testing the model rather than implementation details.

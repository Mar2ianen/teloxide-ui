# Teloxide boundary

`teloxide-ui` is a standalone crate.

This document defines when a discovery made while building the UI runtime should
be implemented in `Mar2ianen/teloxide-fork` instead of locally.

## The test

A primitive is a candidate for teloxide when all of these are true:

1. It is useful to a normal teloxide bot that does not depend on `teloxide-ui`.
2. Its API can be explained entirely in Telegram/request/runtime terms.
3. It does not know about `Ui`, `Component`, `View`, `Action`, `UiStore`, UI
   revisions, or render policy.
4. Moving it to teloxide removes duplicated generic transport/runtime logic.
5. Its lifecycle belongs close to Telegram request execution.

If any item fails, keep it in `teloxide-ui` until there is stronger evidence.

## Clearly teloxide-owned

Examples:

- Bot API 10.3 request/type coverage;
- Rich Message block/button types;
- validation of Telegram method constraints;
- ephemeral message request types;
- generic edit/send request helpers;
- outbound admission, rate limiting, priorities, ordering lanes, and
  latest-wins request coalescing;
- generic durable outbound outbox;
- Drafter streaming lifecycle;
- native Stop-button handling;
- delivery-certainty classification.

## Clearly teloxide-ui-owned

Examples:

- semantic `Ui<Action>` tree;
- component/view traits;
- action registry;
- callback capability-token format;
- stale action policy;
- actor binding policy;
- `UiStore`;
- view revisions;
- desired/rendered UI revisions;
- surface render workers;
- renderer selection;
- legacy-vs-rich rendering policy;
- UI effects.

## Likely candidates discovered during implementation

### Typed ephemeral message identifier

The fork currently exposes ephemeral edit IDs as raw `i32` request fields.

If UI code repeatedly needs a semantic newtype and the same type is useful to
ordinary teloxide users, a `EphemeralMessageId` newtype may belong in
`teloxide-core`.

The UI crate should not create a competing Telegram ID type if the generic
teloxide type is accepted upstream/fork-side.

### Generic editable target

If several unrelated teloxide features need a common abstraction for
`message_id` / `inline_message_id` / ephemeral target addressing, a transport
level target type might belong in teloxide.

Do not move `Surface` itself merely because it contains similar fields:
`Surface` also carries UI projection semantics and ordering identity.

### Generic rich-message helpers

Constructors/validation that make `InputRichMessage` itself safer or easier for
all users belong in teloxide.

Semantic layout components (`Ui::button_row`, `Ui::table`) stay here.

### Scheduler hooks

If UI projection needs a generic scheduler operation that is naturally useful
for other request producers, extend the outbound scheduler.

Do not bypass the scheduler by implementing a UI-only rate limiter.

### Drafter adapters

Capabilities needed by any custom Drafter backend belong in teloxide.

An `Effect::Draft` or UI-specific bridge belongs here.

## Dependency strategy

Early development pins a known full fork revision for reproducibility.

The current pinned public revision contains the needed Rich Message types but
does not yet expose the fork's `outbound` feature in the `teloxide` manifest.
Until that generic wiring lands at a published revision, local checks use a
temporary Cargo patch to a fork checkout that already contains it. This is an
integration prerequisite, not a reason to copy the scheduler into
`teloxide-ui`.

Before publishing `teloxide-ui` to crates.io, the dependency story must be
resolved deliberately. Preferred outcomes:

1. required primitives land in a released teloxide version; or
2. a published fork package/version provides them with a crates.io-compatible
   dependency declaration.

A floating git branch is not an acceptable stable release dependency.

## Change process

When a UI task reveals a missing generic teloxide primitive:

1. document the missing primitive in the UI issue/PR;
2. prove that the API is UI-independent;
3. implement/test it in teloxide;
4. pin/update the teloxide dependency;
5. keep only the UI adapter in this crate.

Do not combine a large teloxide refactor and a large UI feature into one opaque
change.

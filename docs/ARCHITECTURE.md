# Architecture

This document defines the runtime model of `teloxide-ui`.

The public API may change while the crate is experimental. The invariants in
this document should change only deliberately.

## 1. System boundary

`teloxide-ui` is an application/runtime layer above teloxide.

```text
Application
    ↓
teloxide-ui
    ↓
teloxide
    ↓
Telegram Bot API
```

It consumes teloxide types and request/runtime facilities. It must not fork a
parallel Telegram transport stack.

## 2. Core model

The conceptual application model is:

```text
(State, Action) → Transition
State → Ui<Action>
```

`Transition` may update state and request explicit effects.

`Ui<Action>` is a semantic desired representation, not a remote widget object.

The current MVP exposes these concrete building blocks:

```rust,ignore
let registry = ActionRegistry::new();
let renderer = RichRenderer::new(registry.clone());
let rendered = renderer.render(
    &Ui::column().push(
        Ui::button_row().push(Ui::button("Next", Action::Next)),
    ),
    RenderContext::new(view_id, revision),
)?;
```

`ActionRegistry` keeps typed actions behind opaque `ActionToken` values and
checks actor, view, expiry, and stale-revision policy. `InMemoryUiStore` gives
the MVP a compare-and-set state transition boundary. `RichRenderer` produces
an `InputRichMessage`; its semantic table node can contain interactive cells;
`SurfaceWorker` sends that complete representation through a caller-owned
teloxide `OutboundQueue`.

## 3. State

Application state is authoritative on the server.

A stored view record is expected to contain at least:

```rust,ignore
struct ViewRecord<S> {
    id: ViewId,
    revision: Revision,
    state: S,
    surface: Surface,
}
```

Persistent implementations should provide compare-and-set or equivalent
transactional revision checks.

### Revision invariant

For one logical view:

```text
revision(n + 1) > revision(n)
```

An action referring to an older revision is stale and must follow an explicit
policy.

Suggested initial policy:

```rust
enum StalePolicy {
    Reject,
    ApplyToLatest,
    Idempotent,
}
```

`Reject` is the safe default for stateful controls.

## 4. Actions

Application code uses typed actions.

Telegram receives a compact transport token.

The default stateful flow is opaque:

```text
callback_data
    ↓
ActionToken
    ↓
server ActionRecord
```

A conceptual action record:

```rust,ignore
struct ActionRecord<A> {
    token: ActionToken,
    view_id: ViewId,
    revision: Revision,
    action: A,
    actor: ActorPolicy,
    expires_at: Option<Instant>,
}
```

### Why not encode state in callback data?

Because it:

- creates size pressure;
- duplicates server truth;
- makes stale state difficult to reason about;
- encourages trusting client-visible input;
- complicates invalidation and actor binding.

An inline codec may be supported for explicitly stateless actions, but it must
not weaken the stateful contract.

## 5. View and UI tree

A view is pure:

```text
view(state) → Ui<Action>
```

The UI tree exists for server-side composition.

It may model semantic elements such as:

- text;
- paragraph;
- heading;
- button;
- button row;
- table cells containing text or buttons;
- table;
- blockquote/callout;
- collapsible details;
- media;
- fragment/column.

It must not promise capabilities Telegram does not expose:

- arbitrary subtree patching;
- CSS;
- pixel sizing;
- layout measurement;
- client-side hooks;
- client-owned component state.

## 6. Renderer

A renderer converts the semantic tree into a Telegram representation.

Initial target:

```text
Ui<Action>
    ↓
RichRenderer
    ↓
InputRichMessage
```

A later legacy renderer may produce plain text plus
`InlineKeyboardMarkup`.

Button labels remain semantic until this boundary. `ButtonLabel::Plain` is
rendered as text, while `ButtonLabel::CustomEmoji` becomes a Telegram
`RichTextObject::CustomEmoji` with its explicit alternative text. The UI crate
does not expose Telegram's wire object as the application model.

### Renderer invariant

Application semantics are expressed as the complete desired representation.

Diffing is optional optimization.

A renderer may skip a no-op or choose a cheaper edit operation, but the
observable result must be equivalent to replacing the current surface with the
desired representation.

## 7. Surface

A surface is an independently addressable Telegram projection target.

Initial conceptual variants:

```rust
enum Surface {
    Message {
        chat_id: ChatId,
        message_id: MessageId,
    },
    Inline {
        inline_message_id: String,
    },
    Ephemeral {
        chat_id: ChatId,
        receiver_user_id: UserId,
        ephemeral_message_id: i32,
    },
}
```

The exact public shape may evolve with teloxide types.

The MVP implements all three address forms above. `SurfaceWorker` allocates one
teloxide serial lane and one latest-wins coalescing key per surface. It does not
own a second rate limiter, transport client, or durable outbox.

### Surface ordering invariant

For a surface with desired revisions:

```text
R41 < R42 < R43
```

the final visible projection must never end at `R41` or `R42` after `R43`
has successfully projected.

All Telegram mutations for one surface therefore pass through a shared ordering
lane.

## 8. State transition vs projection

The runtime intentionally separates authoritative commit from Telegram
projection.

```text
load
→ validate
→ transition
→ commit revision
→ release state lock
→ render
→ schedule Telegram projection
```

The state lock/transaction is never held across the Telegram request.

This means Telegram failure after commit does not undo the domain transition.

A persistent runtime may track:

```text
desired_revision
rendered_revision
```

and retry missing projection separately.

## 9. Coalescing

Pending representations of the same surface are naturally latest-wins.

Example:

```text
R41 queued
R42 queued
R43 queued
```

If none has started, the runtime may keep only `R43`.

If `R41` is already in flight, it is not retroactively erased. The surface lane
ensures any later successful `R43` projection happens after it.

This behavior should use teloxide's generic outbound scheduler rather than a
second custom scheduler.

## 10. Callback handling

A callback path is conceptually:

```text
CallbackQuery
    ├─→ ACK path
    └─→ action path
          ↓
       resolve token
          ↓
       validate actor/expiry/revision
          ↓
       load state
          ↓
       transition
          ↓
       persist
          ↓
       render/effects
```

Callback acknowledgement must not wait for arbitrary rendering latency.

The runtime should provide a safe default empty acknowledgement and allow an
explicit toast or alert result.

The callback dispatcher and automatic ACK path remain application-owned in the
core MVP. `examples/chess.rs` demonstrates the intended safe ordering: it sends
the empty ACK through the shared outbound queue before resolving the action or
starting projection.

## 11. Effects

Effects are explicit operations produced by a transition.

Candidate effect set:

```rust,ignore
enum Effect<A> {
    Render(Ui<A>),
    RenderAt(SurfaceId, Ui<A>),
    Send(Ui<A>),
    Ephemeral(Ui<A>),
    Delete,
    Toast(String),
    Alert(String),
    Draft(DraftSpec),
}
```

The exact API is not fixed.

Effects must not be hidden in view rendering.

## 12. Ephemeral surfaces

Ephemeral messages have different addressing/edit APIs from ordinary messages
and therefore deserve an explicit surface variant.

Good use cases:

- per-user settings;
- drill-down menus from shared messages;
- confirmations;
- temporary detail views.

Ephemeral projection must not be the sole durable record of a domain mutation.

## 13. Drafter integration

Drafter is not the base rendering engine.

It remains a specialized lifecycle runtime for streamed/partial generation.

Integration should be explicit through effects/adapters.

Possible flow:

```text
Action
  ↓
Transition: state = Generating
  ↓
ordinary Ui render
  +
Draft effect
      ↓
teloxide Drafter
      ↓
native draft / edit-in-place streaming
```

Native Stop handling stays owned by Drafter/teloxide.

## 14. Durable projection

A later persistent runtime may atomically commit:

```text
new state
new desired revision
outbox projection intent
```

Then an outbox worker performs Telegram projection.

Because the renderer sends complete desired representations, re-delivery is
much easier to reason about than imperative UI mutations such as "increment
counter by one".

At-least-once transport still requires explicit delivery semantics.

## 15. Failure classes

The runtime should distinguish at least:

- invalid/unresolvable action token;
- expired token;
- unauthorized actor;
- stale revision;
- store conflict;
- local render/validation failure;
- scheduler admission failure;
- Telegram request failure;
- ambiguous delivery result.

Do not collapse these into one generic callback error.

## 16. Observability

Runtime instrumentation should use IDs and metadata, not full private payloads
by default.

Useful dimensions:

- view id;
- surface kind;
- desired/rendered revision;
- action type/tag;
- stale/authorized decision;
- scheduler wait;
- render duration;
- Telegram result class.

Avoid logging bot tokens, opaque callback secrets, or full user content by
default.

## 17. Testing strategy

Concurrency and state invariants matter more than snapshot cosmetics.

Required test families:

- action token validation;
- revision/stale policy;
- concurrent state transitions;
- surface render ordering;
- coalescing;
- projection failure after commit;

The current unit suite covers token format/registry decisions, compare-and-set
store behavior, renderer validation, and stable per-surface lane allocation.
End-to-end Telegram projection and callback dispatch remain follow-up tests.
- ephemeral addressing;
- callback ACK latency independence;
- renderer structural validation.

A reference chess UI is valuable because an 8×8 board stresses button rows,
stateful actions, stale moves, and rapid per-surface rerendering.

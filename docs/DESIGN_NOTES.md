# Design notes

This file holds decisions that are useful context but are not yet guaranteed
public API.

## HTMX as a reference

The useful idea from HTMX is not HTML syntax. It is the hypermedia control
flow:

```text
event
→ server request
→ server computes next representation
→ target replacement
```

In Telegram:

```text
callback
→ typed action
→ state transition
→ full desired Ui
→ Telegram edit
```

The important difference is that Telegram does not expose a DOM target tree.
The natural mutable target is a message-like surface.

## Elm/TEA influence

`State + Action + view()` is intentionally similar to The Elm Architecture,
but there is no client-side runtime.

Effects remain explicit so that render functions are deterministic and easy to
test.

## Why full desired representation?

An imperative API such as:

```text
increment counter label
disable button 4
replace table row 2
```

creates difficult replay and partial-failure semantics.

A declarative API says:

```text
the desired surface at revision 42 is R42
```

This is compatible with:

- retry;
- latest-wins coalescing;
- durable outbox projection;
- no-op detection;
- renderer optimization.

## Why opaque actions first?

Telegram callback data is a transport constraint and an untrusted input
channel. Opaque tokens decouple domain actions from that constraint and permit:

- expiry;
- actor binding;
- revision binding;
- invalidation;
- compact payloads;
- future token format changes.

Inline action encoding can be added later as an explicit optimization for
stateless flows.

## Why no UI macros initially?

Proc macros make attractive examples but freeze vocabulary too early.

The builder/typed model should survive real chess/settings/pagination examples
before a `ui!` syntax is designed.

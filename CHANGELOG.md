# Changelog

All notable changes to this project will be documented in this file.

The project is pre-release. Public API compatibility is not guaranteed before
the first explicitly stabilized release.

## Unreleased

### Added

- Initial repository architecture.
- MIT license.
- Project and agent documentation.
- Initial semantic UI, surface, effect, and component type skeletons.
- Initial CI configuration.
- `ViewId` and monotonic `Revision` primitives.
- Opaque `ActionToken` values with registry-backed actor, TTL, view, and stale
  revision validation.
- Optimistic-concurrency `InMemoryUiStore`.
- Rich Message rendering for the initial semantic UI nodes.
- `SurfaceWorker` with one serial/latest-wins outbound lane per surface.
- Chess reference application with server-side board state, opaque callbacks,
  immediate ACK, CAS transitions, and same-message Rich projection.
- Pinned the teloxide dependency to fork commit `67432b14`, where `outbound`
  is an explicit opt-in feature.

### Compatibility

- Set the current MSRV and CI toolchain to Rust 1.88.

### Design

- Established `teloxide-ui` as a standalone crate depending on teloxide.
- Defined server-authoritative state, pure view rendering, surface ordering,
  explicit stale-action policy, and callback capability tokens as core
  invariants.

# Changelog

All notable changes to this project will be documented in this file.

The project is pre-release. Public API compatibility is not guaranteed before
the first explicitly stabilized release.

## Unreleased

- Fix chess custom-emoji metadata on the first state update: native selected,
  legal, and capture overlays now retain their declared `▫️` alternative text,
  while ordinary pieces keep the same metadata-matched palette in group and
  private projections.
- Add an explicit PVP autostart mode for group smoke tests.

### Added

- `lazychess` UCI/analysis integration for Stockfish-backed computer games;
  the adapter captures the best move and primary-line evaluation while the
  authoritative application rules remain in `cozy-chess`.
- Chat-aware chess modes: private `/chess` versus Stockfish, group `/chess`
  for two players, explicit `/chess pvp` and `/chess bot` selectors, and a
  server-validated Black-seat claim.
- Group chess now projects White to the shared message and Black to a targeted
  ephemeral Rich Message; each surface keeps its own board orientation while
  all projections continue to derive from one authoritative game state.
- PVP turn status now shows the current player's bounded Telegram profile name
  rather than the `@username` handle, with a color fallback when profile
  identity is unavailable.
- Chess callbacks now show concise rejection feedback for stale, wrong-side,
  and illegal moves; a human move is still projected if a later Stockfish
  calculation fails.
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
- Semantic Rich Message tables with text, empty, and interactive button cells.
- Semantic Rich Message blockquotes and collapsible details blocks.
- `SurfaceWorker` with one serial/latest-wins outbound lane per surface.
- Separate `teloxide-ui-chess` reference application with server-side board
  state, opaque callbacks, immediate ACK, CAS transitions, and same-message
  Rich projection.
- `cozy-chess` integration for complete legal-move and game-status handling;
  chess domain rules remain outside the `teloxide-ui` runtime.
- Chess moves now reject positions that leave the moving side's king in check
  and report checkmate or stalemate in the stable one-line status block.
- Semantic `ButtonLabel` values with Rich Message custom-emoji rendering; the
  chess example uses a published flat 2D set for pieces and cell states, with
  valid Unicode emoji fallbacks.
- Native `TableCell::header` rendering for Telegram header-cell backgrounds;
  the chess reference uses it to keep checkerboard cells adjacent without
  inline full-cell sprite gaps.
- Rebuilt the chess visual palette around Telegram's native compact-table
  backgrounds and 52 transparent 100×100 overlays. ImageGen remains limited to
  the flat 2D piece source sheet; locally generated overlays provide pieces,
  selection, and legal/capture markers without putting a second background in
  each cell. Light cells use native header backgrounds so adjacent squares stay
  flush with the reference.
- Pinned the teloxide dependency to fork commit `a6092220`, where `outbound`
  is an explicit opt-in feature.

### Compatibility

- Set the current MSRV and CI toolchain to Rust 1.88.

### Design

- Established `teloxide-ui` as a standalone crate depending on teloxide.
- Defined server-authoritative state, pure view rendering, surface ordering,
  explicit stale-action policy, and callback capability tokens as core
  invariants.

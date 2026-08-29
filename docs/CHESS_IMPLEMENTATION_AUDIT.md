# Chess implementation audit

This is a post-MVP review of the chess application history and of the
hardcoded-versus-configurable boundary. It is based on the repository commits
from `1a1f0a7` through `512cb2d`, the reference screenshots, and the current
runtime contract.

## Executive result

The final visual direction is correct: a compact native Telegram table owns
the checkerboard background, and each of the 64 cells carries one transparent,
fixed-size overlay. The earlier visual regressions came from treating a board
cell as a complete image, including its background. Telegram then rendered
the image as a button-sized object with gaps and changed row metrics when the
asset changed.

The application boundary is also now correct: chess is a separate workspace
package and does not belong in `teloxide-ui`. The next boundary to improve is
inside that package: rules/state, engine integration, rendering, and Telegram
transport should become separate modules or crates before persistence and
analysis features grow.

## What should have happened first

### 1. Freeze the Telegram rendering contract before generating assets

The first assets and implementations (`1a1f0a7`, `8aa5fa4`, and the later
`cbfac88` experiment) encoded full colored cells. That was an understandable
approximation from the screenshots, but it was the wrong primitive for the
actual Rich Message renderer. The reference contract should have been written
before producing a large emoji set:

```text
one table cell
  = Telegram native table background
  + one transparent 100x100 custom-emoji overlay
  + one server-side action token
```

This would have prevented the large `chess-emoji-reference-v2` asset family
and the repeated reversions between full-cell and native-background models.
The native-background approach eventually landed in `c914c42` and was made
reproducible in `b97d987`.

### 2. Validate structure, not only source images

The important invariant was not “the PNG looks like a square”. It was:

- every board row has the same number of cells;
- all 64 cells have the same transparent overlay geometry;
- light cells use native table headers and dark cells use ordinary cells;
- selecting a piece does not add a node, row, or line;
- all cells retain an action token;
- the projection order is stable.

The height fix in `612226a` and the structural test in `512cb2d` came after
the visual feedback. These tests should have existed before the first live
message. The next test layer should snapshot the rendered Rich Message tree
and the action-token count for base, selected, legal-move, capture, and
finished states.

### 3. Keep domain code separate from transport from the first feature slice

`d7f763f` correctly moved the application out of the UI runtime, but it came
after the chess implementation had already accumulated UI and transport
assumptions. The next iteration should keep these layers explicit:

```text
chess-core       board, players, modes, transitions, legal moves
chess-engine     Stockfish/UCI capability and score mapping
chess-render     Ui<Action> projection and palette
telegram-adapter callback ACK, CAS, SurfaceWorker, message lifecycle
```

This is not a reason to move anything back into `teloxide-ui`. It is a reason
to make the application crate internally composable before adding clocks,
PGN export, persistence, or engine analysis UI.

### 4. Model players before opening callbacks to a group

The original creator-bound actor policy was sufficient for a private demo but
not for a group game. Two-player mode needs explicit server-side seats and
transition checks. The current implementation does that with `white_player`,
`black_player`, and a CAS-protected `JoinBlack` action. The same rule should
be retained if the state is moved to a persistent store: the Telegram message
must never be the authority for who owns a side.

### 5. Treat the engine as an effect, never as part of `view()`

Stockfish calculation is blocking and external. The current adapter uses the
broader MIT `lazychess` crate for UCI communication, analysis records, scores,
MultiPV support, FEN, and PGN capabilities, while `cozy-chess` remains the
authoritative move/status model already used by the application. The engine
process is serialized behind one capability and called from
`spawn_blocking`; state is committed before the call and compared again before
the engine result is committed. This preserves the store/network ordering
invariant and discards an engine result if Undo, Reset, or another action won
the race.

## Commit review

| Commit | What it taught us | Better ordering for a fresh implementation |
| --- | --- | --- |
| `1a1f0a7` | A flat 2D palette was useful, but the first assets mixed piece art with cell backgrounds. | Define the native-table/transparent-overlay contract first; generate only the piece source and transparent state overlays. |
| `8aa5fa4` | Full-cell assets multiplied quickly across colors, pieces, and interaction states. | Do not duplicate the background in each asset when Telegram already owns that layer. |
| `cf69ba3` | The Rich Message model and renderer had to learn the real table primitive. | Implement and test the semantic table node before using it as a chess board. |
| `54f443e` / `cbfac88` | Compact-table experiments were valuable, but the implementation oscillated because the wire-level contract was not frozen. | Keep experiments isolated and attach a rendered-tree test to each candidate. |
| `db302bf` / `d30026b` | Cell scale and projection order are product behavior, not cosmetic details. | Treat geometry, coordinate gutters, and projection order as explicit rendering invariants. |
| `c914c42` | Native table backgrounds matched the reference and removed the source of the gaps. | This should have been the first production rendering model. |
| `76bfc1e` | Legal move generation must reject moves exposing the king. | Delegate all legality/status checks to one tested rules crate before UI work. |
| `d7f763f` | Chess belongs in a separate package over `teloxide-ui`. | Establish the package boundary before adding application-specific nodes or handlers. |
| `612226a` | Dynamic details/status content can change the whole Telegram message height. | Keep a fixed projection skeleton and test line/row stability for each state. |
| `b97d987` / `512cb2d` | Native headers plus transparent overlays are the stable solution; the test protects the alternating-cell contract. | Add golden Rich Message payloads and a live-client smoke test alongside the structural test. |

The history was not wasted: each failed visual direction exposed a missing
contract. The main avoidable cost was creating and publishing assets before the
Telegram node semantics and screenshot acceptance criteria were fixed.

## What belongs in code

These are application semantics and security invariants. They must not be
editable by the Telegram client or inferred from callback labels:

- an 8×8 board and the mapping between squares and coordinates;
- legal transitions, promotion policy, check/checkmate/stalemate handling;
- `GameMode`, player-seat ownership, and which side Stockfish controls;
- callback token validation, view/revision matching, stale rejection, and CAS;
- immediate callback acknowledgement before engine/render work;
- the fixed Rich Message table shape and the stable order of board, status,
  history, and controls;
- the palette index order and the fact that every cell has equal overlay
  geometry and a server-side action;
- the rule that engine results are validated against the current board before
  commit;
- no state-store lock across a Telegram or Stockfish operation.

“Hardcoded” here means a tested invariant, not a secret and not a deployment
identity.

## What belongs in configuration

These values vary between machines, bots, deployments, or experiments and
should not require a source edit:

- `TELOXIDE_TOKEN`;
- `STOCKFISH_PATH`;
- `STOCKFISH_MOVETIME_MS` (bounded by the application) or
  `STOCKFISH_DEPTH`;
- future engine options such as Threads, Hash, Elo/skill, and MultiPV;
- `TELOXIDE_CHESS_AUTOSTART_CHAT_ID`;
- the default mode, if a deployment wants a different `/chess` policy;
- published custom-emoji set identifiers when a deployment uses another set.

The checked-in `assets/chess-emoji-native/ids.txt` is intentionally a
reproducible build manifest for the current published palette. It is not a
secret. If multiple deployments need different palettes, load a validated
manifest or configuration at startup rather than scattering Telegram IDs
through the renderer.

## What must never be hardcoded or trusted from the client

- bot tokens, personal credentials, or private chat identifiers;
- the current board, revision, player identity, or engine result in
  `callback_data`;
- authority based only on a rendered label such as “Join as Black”;
- a Stockfish move without checking that it is legal for the committed FEN;
- a successful Telegram edit as proof that the domain transition succeeded;
- a visual screenshot as proof that action routing and stale rejection work.

Callback data is only an opaque capability. The server record remains the
authority for action, actor, view, revision, and expiry.

## Recommended next refactor

Before adding evaluation panels, clocks, opening books, or persistent games:

1. Extract a small `teloxide-ui-chess-core` library with `ChessState`,
   `GameMode`, seats, transitions, and a trait-free deterministic test suite.
2. Extract a `ChessEngine` capability trait and keep the `lazychess` adapter
   behind it; add a fake engine for CAS/cancellation tests.
3. Add rendered-tree snapshots for the five board visual states and both modes.
4. Add a persistent store with desired/rendered revision tracking.
5. Only then expose score/MultiPV data in the UI; keep the reference board
   projection fixed-height and make any analysis view an explicit stable block.

That sequence keeps the reference appearance intact while allowing the engine
and multiplayer features to grow without turning the UI runtime into a chess
runtime.

# teloxide-ui-chess

The reference chess application for [`teloxide-ui`](../..). This package is
an application, not part of the declarative UI runtime: chess rules, board
state, emoji assets, and the Telegram bot entry point live here.

The authoritative move legality and game status come from
[`cozy-chess`](https://github.com/analog-hors/cozy-chess). Engine integration
uses [`lazychess`](https://crates.io/crates/lazychess) as the Rust UCI/analysis
adapter. `teloxide-ui` remains responsible only for the server-driven UI
primitives, action tokens, rendering, and surface ordering.

The board uses the published
[`teloxide_ui_chess_native_v2_by_testteloxideui_bot`](https://t.me/addemoji/teloxide_ui_chess_native_v2_by_testteloxideui_bot)
custom-emoji set. The board background is Telegram's native compact-table
checkerboard: light squares are native header cells and dark squares are
ordinary cells. Its 100×100 transparent overlays provide the flat pieces,
selection border, and legal/capture markers. ImageGen is used only for the
transparent 2D piece source sheet; the table background and overlays are
generated locally. Every empty square still has an interactive transparent
label, keeping selection from changing the table's row height.

## Game modes

- In a private chat, `/chess` starts a game as White against Stockfish.
- In a group or supergroup, `/chess` starts a two-player game. The author is
  White; the first other participant who presses `Join as Black` takes Black.
- `/chess pvp` explicitly selects two-player mode.
- `/chess bot` or `/chess stockfish` explicitly selects Stockfish mode.

The mode and player seats are server-side state. A callback never carries a
user id, side, board position, or move as trusted data. The group join action
is open at the token layer so a participant can claim the empty seat; the
transition validates the seat and all subsequent moves against the current
server state.

## Stockfish setup

The `lazychess` crate provides the broader UCI layer used here: best moves,
bounded search by time or depth, engine scores, MultiPV, FEN/PGN support, and
analysis records. It does not bundle Stockfish itself. Install an official
Stockfish binary separately and configure its path:

```bash
export STOCKFISH_PATH=/path/to/stockfish
export STOCKFISH_MOVETIME_MS=350  # 100..5000, default 350
# Optional: depth takes precedence over movetime.
export STOCKFISH_DEPTH=12
```

If `STOCKFISH_PATH` is omitted, the bot looks for `stockfish` in `PATH`. If no
engine is available, `/chess` in a private chat explains the configuration
problem and `/chess pvp` remains available. The Rust adapter and this project
are MIT; the separately installed Stockfish binary is distributed under its
own GPLv3 license and must be handled according to that license.

## Run

Set `TELOXIDE_TOKEN` and run from the repository root:

```bash
cargo run -p teloxide-ui-chess
```

For a one-shot demo message in a known chat, also set
`TELOXIDE_CHESS_AUTOSTART_CHAT_ID` and configure Stockfish first. The
autostart demo uses engine mode.

The implementation review and the hardcoded-versus-configurable boundary are
in [`docs/CHESS_IMPLEMENTATION_AUDIT.md`](../../docs/CHESS_IMPLEMENTATION_AUDIT.md).

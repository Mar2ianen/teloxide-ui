# Chess reference application

`crates/teloxide-ui-chess` is the first application built on top of
`teloxide-ui`. It is a separate workspace package: chess rules, game state,
emoji palette, and the Telegram bot entry point do not belong to the UI
runtime.
It reproduces the important interaction model of Telegram's Rich Message
chess demonstration: one message contains the board, every cell is a native
Rich Message button, a click arrives as a callback, and the bot edits the same
message with the next complete representation. The example uses a compact,
non-striped Rich Message table. Each board cell is an interactive table cell
using a transparent overlay from
[`teloxide_ui_chess_native_v2_by_testteloxideui_bot`](https://t.me/addemoji/teloxide_ui_chess_native_v2_by_testteloxideui_bot).
Telegram's native table background supplies the checkerboard: light squares
are native header cells and dark squares are ordinary cells, so adjacent
squares remain flush. Selection borders and legal/capture markers are
generated locally; ImageGen is used only for the transparent 2D piece source
sheet in [`assets/chess-emoji-native/`](../assets/chess-emoji-native/).
Because empty and occupied cells use the same transparent overlay geometry, the
board does not grow or jump when a piece is selected. The piece-only entries
use their matching `⚪`/`⚫` metadata, while every stateful overlay preserves
the manifest's `▫️` alternative text. That alternative is part of Telegram's
custom-emoji contract, not a place for the application to substitute a green,
red, or yellow marker.

All projections use the original piece-only palette, whose `⚪`/`⚫` fallback
metadata matches the custom emoji. Empty base cells use an invisible plain-text
button rather than a `▫️` custom-emoji fallback. This preserves the stable
64-cell action grid without showing a white placeholder when a client cannot
resolve the custom-emoji document.

The reference follows this flow:

```text
/chess
  → create ViewId + server-side ChessState
  → view(state) → Ui<ChessAction>
  → RichRenderer → table cells + opaque ActionToken values
  → sendRichMessage through OutboundQueue

button click
  → resolve token: actor + view + expiry + revision
  → validate local transition
  → answerCallbackQuery immediately (with a short rejection toast when invalid)
  → compare_and_set state transition
  → render new revision
  → SurfaceWorker edits the same Message surface
```

The board is creator-bound when Telegram supplies a message author. State is
never encoded in callback data or button labels. Two callbacks racing on one
revision are resolved by `UiStore::compare_and_set`; only the winner projects
that revision.

The application delegates chess legality and game status to
[`cozy-chess`](https://github.com/analog-hors/cozy-chess). This covers normal
piece movement, captures, promotion, castling, en passant, king safety,
check, checkmate/stalemate detection, and draw rules. The rules engine remains
an application dependency; none of these domain rules are part of the
UI-runtime crate.

For computer play, the application uses
[`lazychess`](https://crates.io/crates/lazychess) as a Rust UCI and analysis
adapter around an externally installed Stockfish binary. The adapter returns
the best UCI move and the deepest primary-line score. The engine call runs in
`spawn_blocking`, while the store is only accessed before and after the call;
the engine never runs from `view()` and no state lock crosses the calculation.

The default modes are intentionally chat-aware: private `/chess` starts
human-as-White versus Stockfish, while group `/chess` starts a two-player
game. `/chess pvp` and `/chess bot` make the choice explicit. In two-player
mode the creator owns White and one other Telegram user can claim Black. A
seat claim is an optimistic state transition, so two simultaneous claims leave
only one committed Black player. In engine mode the external user can never
submit a Black move; only the engine transition can do so. Undo removes the
last human/engine pair in engine mode so the next visible turn remains White.

In a group chat, the shared message is the White-facing projection. When a
second player claims Black, the bot sends that player a targeted ephemeral Rich
Message using the callback query that claimed the seat; that projection shows
Black at the bottom and replaces only that player's callback message. Later
state revisions are projected to the shared message and every registered
ephemeral player surface. `Flip board` changes only the surface that produced
the callback, so White and Black may keep opposite orientations without
putting presentation state into `ChessState`.

The two-player status line uses the current player's Telegram profile name
(`first_name` plus `last_name`), for example `Alice Smith to move`; it does not
use the `@username` handle. Labels are bounded before rendering; if no player
identity is available, the status falls back to `White to move` or
`Black to move`. The piece labels keep the emoji fallbacks associated with the
published custom-emoji set so Telegram accepts the Rich Message payload.

The engine path is configured with `STOCKFISH_PATH`. Search uses
`STOCKFISH_MOVETIME_MS` (100–5000 ms, default 350) unless `STOCKFISH_DEPTH`
(1–32) is set. This keeps deployment choices out of the source while keeping
the rules, callback protocol, fixed board geometry, and authority checks in
code.

## Run it

Create or select a test bot with BotFather, then run the example with its token
in the `TELOXIDE_TOKEN` environment variable:

```bash
export TELOXIDE_TOKEN="$(< /path/to/telegram-token.txt)"
cargo run -p teloxide-ui-chess
```

For a deterministic group/PVP smoke test, set
`TELOXIDE_CHESS_AUTOSTART_MODE=pvp` together with
`TELOXIDE_CHESS_AUTOSTART_CHAT_ID`. The default autostart mode remains
Stockfish.

Open the bot in Telegram and send `/chess`. Use a current Telegram client with
Rich Message support. Stop the local process with Ctrl-C.

The token is intentionally read from the environment and is never part of the
repository or callback payloads.

## Reference behavior

The inspiration is Telegram's [`@RichChessBot`](https://t.me/RichChessBot)
and the Bot API Rich Message/button model described in the [official Bot API
documentation](https://core.telegram.org/bots/api). The reference app keeps
the transport adapter in teloxide and keeps game state, action policy, render
composition, and surface mapping in the application layer.

The board is a compact, non-striped Rich Message table. Its light squares use
Telegram's native header background and its dark squares use the ordinary
table background, matching the reference's tight square grid without inline
background sprites. Transparent 100×100 overlays keep cell metrics stable.
Rank labels are rendered on the left and file labels above and below it,
matching the current compact projection. The
projection order is board, turn status, move history, and controls, matching
the reference. All 64 cells keep a transparent button label even when empty;
only the server-side action semantics decide whether a click is a legal move.
Coordinate gutters, board flipping, undo, finish, and new-game controls are
part of the projection. The view marks the selected piece and
legal destinations, and the server recomputes legality during the callback
transition for both colors. Empty legal destinations use the green overlay
artwork; occupied capture targets use the red overlay artwork. A stale or
illegal callback never changes state. It is acknowledged with a short client
notification instead of being silently dropped, so an empty square, a
wrong-side click, a stale button, or a move that would leave the king in check
has visible feedback without waiting for a render. Successful callbacks are
acknowledged before ephemeral delivery, Stockfish, or Telegram edit work
begins.

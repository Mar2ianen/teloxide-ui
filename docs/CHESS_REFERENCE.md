# Chess reference application

`examples/chess.rs` is the first application built on top of `teloxide-ui`.
It reproduces the important interaction model of Telegram's Rich Message
chess demonstration: one message contains the board, every enabled cell is a
native Rich Message button, a click arrives as a callback, and the bot edits
the same message with the next complete representation. The example uses the
two custom emoji IDs extracted from the supplied empty reference board for
empty cells; Unicode chess symbols remain the deterministic fallback for
pieces.

The reference follows this flow:

```text
/chess
  → create ViewId + server-side ChessState
  → view(state) → Ui<ChessAction>
  → RichRenderer → InputRichMessage + opaque ActionToken values
  → sendRichMessage through OutboundQueue

button click
  → answerCallbackQuery immediately
  → resolve token: actor + view + expiry + revision
  → compare_and_set state transition
  → render new revision
  → SurfaceWorker edits the same Message surface
```

The board is creator-bound when Telegram supplies a message author. State is
never encoded in callback data or button labels. Two callbacks racing on one
revision are resolved by `UiStore::compare_and_set`; only the winner projects
that revision.

The current game rules are deliberately MVP-sized: normal piece movement,
captures, pawn promotion to a queen, turn ownership, and reset. Check,
checkmate, castling, en passant, draw rules, and multi-player matchmaking are
follow-up application work, not UI-runtime primitives.

## Run it

Create or select a test bot with BotFather, then run the example with its token
in the `TELOXIDE_TOKEN` environment variable:

```bash
export TELOXIDE_TOKEN="$(< /path/to/telegram-token.txt)"
cargo run --example chess
```

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

Every board cell is a button. The view marks the selected piece and legal
destinations, and the server recomputes legality during the callback transition
for both colors. A stale or illegal callback never changes state.

# teloxide-ui-chess

The reference chess application for [`teloxide-ui`](../..). This package is
an application, not part of the declarative UI runtime: chess rules, board
state, emoji assets, and the Telegram bot entry point live here.

The rules are provided by [`cozy-chess`](https://github.com/analog-hors/cozy-chess).
`teloxide-ui` remains responsible only for the server-driven UI primitives,
action tokens, rendering, and surface ordering.

The board uses the published
[`teloxide_ui_chess_native_v2_by_testteloxideui_bot`](https://t.me/addemoji/teloxide_ui_chess_native_v2_by_testteloxideui_bot)
custom-emoji set. The board background is Telegram's native compact-table
checkerboard: light squares are native header cells and dark squares are
ordinary cells. Its 100×100 transparent overlays provide the flat pieces,
selection border, and legal/capture markers. ImageGen is used only for the
transparent 2D piece source sheet; the table background and overlays are
generated locally. Every empty square still has an interactive transparent
label, keeping selection from changing the table's row height.

## Run

Set `TELOXIDE_TOKEN` and run from the repository root:

```bash
cargo run -p teloxide-ui-chess
```

For a one-shot demo message in a known chat, also set
`TELOXIDE_CHESS_AUTOSTART_CHAT_ID`.

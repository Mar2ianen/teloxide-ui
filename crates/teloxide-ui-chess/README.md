# teloxide-ui-chess

The reference chess application for [`teloxide-ui`](../..). This package is
an application, not part of the declarative UI runtime: chess rules, board
state, emoji assets, and the Telegram bot entry point live here.

The rules are provided by [`cozy-chess`](https://github.com/analog-hors/cozy-chess).
`teloxide-ui` remains responsible only for the server-driven UI primitives,
action tokens, rendering, and surface ordering.

## Run

Set `TELOXIDE_TOKEN` and run from the repository root:

```bash
cargo run -p teloxide-ui-chess
```

For a one-shot demo message in a known chat, also set
`TELOXIDE_CHESS_AUTOSTART_CHAT_ID`.

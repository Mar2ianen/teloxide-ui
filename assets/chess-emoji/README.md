# Chess custom emoji assets

This directory contains the 20 static 100×100 PNG assets used by the chess
reference application:

- 12 flat 2D Material-style piece icons;
- base light/dark cell icons;
- selected light/dark cell icons;
- legal-move light/dark cell icons;
- capture-target light/dark cell icons.

They are published in the Telegram custom emoji set
[`teloxide_ui_chess_by_testteloxideui_bot`](https://t.me/addemoji/teloxide_ui_chess_by_testteloxideui_bot).
The runtime stores the resulting custom emoji IDs in
[`examples/chess.rs`](../../examples/chess.rs), while each label retains a
Unicode fallback.

The assets were generated as a transparent sprite sheet and cropped to exact
100×100 RGBA PNG files for Telegram's static custom emoji format.

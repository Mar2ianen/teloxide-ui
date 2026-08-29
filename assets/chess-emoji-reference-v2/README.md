# Chess full-cell sprite experiment

This directory contains an alternate set of 104 static 100×100 PNG custom
emoji. The current chess projection does not use this set.

Each file is a complete board cell: the tile background, optional
selection/legal/capture marker, and optional chess piece are composited into
the same sprite. It is useful for testing fixed-size emoji-button cells, but
the reference-oriented projection uses native striped table cells for the
board surface and transparent piece emoji instead.

The assets are published in the
[`teloxide_ui_chess_v2_by_testteloxideui_bot`](https://t.me/addemoji/teloxide_ui_chess_v2_by_testteloxideui_bot)
custom emoji set. Their Telegram IDs are recorded in
[`examples/chess.rs`](../../examples/chess.rs); labels retain valid Unicode
fallbacks for clients that cannot display the custom emoji.

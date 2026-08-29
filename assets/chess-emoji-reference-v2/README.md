# Chess full-cell sprite experiment

This directory contains an alternate set of 104 static 100×100 PNG custom
emoji. It is not used by the current chess projection.

Each file is a complete board cell: the tile background, optional
selection/legal/capture marker, and optional chess piece are composited into
the same sprite. This was useful for testing fixed-size cell labels, but the
official reference is better matched by a native striped table with
transparent piece emoji, which is the approach used by `examples/chess.rs`.

The assets are published in the
[`teloxide_ui_chess_v2_by_testteloxideui_bot`](https://t.me/addemoji/teloxide_ui_chess_v2_by_testteloxideui_bot)
custom emoji set. Their Telegram IDs are recorded in
[`examples/chess.rs`](../../examples/chess.rs); labels retain valid Unicode
fallbacks for clients that cannot display the custom emoji.

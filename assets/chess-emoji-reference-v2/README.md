# Chess full-cell sprite experiment

This directory contains the 104 static 100×100 PNG custom emoji used by the
current chess projection.

Each file is a complete board cell: the tile background, optional
selection/legal/capture marker, and optional chess piece are composited into
the same sprite. The chess example places these labels in a compact striped
Rich Message table, one button per board cell, so the colored tiles stay
adjacent while selection, legal-destination, and capture states remain
server-rendered.

The assets are published in the
[`teloxide_ui_chess_v2_by_testteloxideui_bot`](https://t.me/addemoji/teloxide_ui_chess_v2_by_testteloxideui_bot)
custom emoji set. Their Telegram IDs are recorded in
[`examples/chess.rs`](../../examples/chess.rs); labels retain valid Unicode
fallbacks for clients that cannot display the custom emoji.

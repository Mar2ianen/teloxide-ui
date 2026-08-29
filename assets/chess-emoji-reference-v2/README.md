# Chess full-cell sprite experiment

This directory contains the 104 static 100×100 PNG custom emoji from the
full-cell chess projection experiment. It is retained as an alternate palette;
the current reference projection uses native table-cell backgrounds with
transparent piece and marker emoji.

Each file is a complete board cell: the tile background, optional
selection/legal/capture marker, and optional chess piece are composited into
the same sprite. The experimental projection placed these labels in a compact
striped Rich Message table, one button per board cell. That approach leaves
an inline emoji-sized gap inside each table cell on some Telegram clients, so
it is not the current reference projection.

The assets are published in the
[`teloxide_ui_chess_v2_by_testteloxideui_bot`](https://t.me/addemoji/teloxide_ui_chess_v2_by_testteloxideui_bot)
custom emoji set. Their Telegram IDs are recorded in
[`examples/chess.rs`](../../examples/chess.rs); labels retain valid Unicode
fallbacks for clients that cannot display the custom emoji.

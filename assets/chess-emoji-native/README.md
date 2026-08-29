# Native chess board palette

This directory contains the assets used by the `teloxide-ui-chess` reference
application.

- `generated-piece-sheet.png` is the transparent flat 2D source sheet made
  with ImageGen. ImageGen was used only for the twelve chess pieces.
- `pieces/` contains the twelve locally cropped and corrected piece sources.
- `overlays/` contains 52 locally composited transparent 100×100 overlays:
  four visual states and thirteen piece/empty variants per state.
- `ids.txt` stores the Telegram custom-emoji IDs in renderer order:
  base, selected, legal, capture; empty, black pieces, then white pieces.

The checkerboard itself is not an image. It is Telegram's native compact table
background: light squares are marked as native header cells and dark squares
remain ordinary cells. This is what keeps adjacent squares flush like the
official Rich Text Chess reference. The transparent overlays provide pieces,
selection borders, and legal/capture markers without introducing a second
background or a gap between cells.

The published set is
[`teloxide_ui_chess_native_v2_by_testteloxideui_bot`](https://t.me/addemoji/teloxide_ui_chess_native_v2_by_testteloxideui_bot).

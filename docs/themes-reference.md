# Theme references (ADR-0008)

Facts gathered from sources to reproduce colours and the look of elements. Attribute byte = background << 4 |
foreground, standard 16 colours (0 black, 1 blue, 2 green, 3 cyan, 4 red, 5 magenta, 6 brown, 7 light gray,
8 dark gray, 9 light blue, A light green, B light cyan, C light red, D light magenta, E yellow, F white).

## The console's 16 colours (key `consolePalette` of a theme)
A theme's colour names are numbers; what they look like is set with `consolePalette`, applied to the console itself.
| name | FAR (`colormix.cpp` `DefaultPalette`) | VGA (DOS: NC, DN; the program in DOSBox) |
|---|---|---|
| black | `#000000` | `#000000` |
| blue | `#000080` | `#0000aa` |
| green | `#008000` | `#00aa00` |
| cyan | `#008080` | `#00aaaa` |
| red | `#800000` | `#aa0000` |
| magenta | `#800080` | `#aa00aa` |
| brown | `#808000` | `#aa5500` |
| lightgray | `#c0c0c0` | `#aaaaaa` |
| darkgray | `#808080` | `#555555` |
| lightblue | `#0000ff` | `#5555ff` |
| lightgreen | `#00ff00` | `#55ff55` |
| lightcyan | `#00ffff` | `#55ffff` |
| lightred | `#ff0000` | `#ff5555` |
| lightmagenta | `#ff00ff` | `#ff55ff` |
| yellow | `#ffff00` | `#ffff55` |
| white | `#ffffff` | `#ffffff` |

## Turbo Vision 2.0 (the look of DOS Navigator; magiblot/tvision, master, 2026-11-08)
Files: `source/tvision/tvtext1.cpp` (glyphs), `tbutton.cpp`, `tframe.cpp`, `tcluster.cpp`, `tcheckbo.cpp`,
`tradiobu.cpp`, `tscrlbar.cpp`, `tview.cpp`, `include/tvision/app.h`, `include/tvision/dialogs.h`.

Glyphs (CP437 -> Unicode):
- Button shadow: `▄` (right column, title row 0), `█` (right column, other rows), `▀` (bottom row, from column
  2). The left column of a normal button is drawn in the shadow colour. Pressed: the button moves one column
  right onto its shadow for 100 ms, then the command runs. Markers `[` `]` and `»` `«` (default) only when
  `showMarkers` is on (off by default): the default button differs by colour only.
- Check box ` [ ] `, mark `X`; radio button ` ( ) `, mark `•`.
- Window frame: active double (`╔═╗║╚╝`), passive single; close icon `[■]` at column 2, zoom `[↑]` / `[↕]` at
  width-5; drag corner `─┘`. Menu box single (`┌─┐│└┘├┤`).
- Scroll bar: `▲` `▼` (`◄` `►`), track `▒`, thumb `■`, empty track `▓`.
- Window shadow: 2 columns right, 1 row down, attribute 0x08 (dark gray on black).

Gray dialog palette (cpAppColor entries 32-63):
| element | byte | colours |
|---|---|---|
| frame passive / active / icons | 70 / 7F / 7A | black / white / light green on light gray |
| static text, label | 70 | black on light gray |
| label selected / shortcut | 7F / 7E | white / yellow on light gray |
| button normal / default / focused | 20 / 2B / 2F | black / light cyan / white on green |
| button shortcut / disabled | 2E / 78 | yellow on green / dark gray on light gray |
| button shadow | 70 | black on light gray |
| check/radio normal / selected / shortcut / disabled | 30 / 3F / 3E / 38 | black / white / yellow / dark gray on cyan |
| input line normal / selected / arrows | 1F / 2F / 1A | white on blue / white on green / light green on blue |
| list normal / focused / selected / divider | 30 / 2F / 3E / 31 | on cyan / white on green / yellow on cyan / blue on cyan |
| scroll bar | 13 | cyan on blue |

Menus and status line (entries 2-7): normal 70, disabled 78, shortcut 74 (red on light gray), selected 20,
selected disabled 28, selected shortcut 24 (red on green).

## DOS Navigator 1.51 (github.com/DOS-Navigator/DNR, `src/pascal/DNAPP.PAS`, `FLPANEL.PAS`)
- Palettes `CColor` (default), `CBlackWhite`, `CMonochrome`; the user's DN.CFG and `/P` palette files override.
- `CColor` 1-64: `71 70 78 74 20 28 24 17 1F 1A 31 31 78 7F 1F 31 | 3F 3A 13 13 3E 21 3F 70 7F 7A 13 13 70 7F 7E 70 |
  7F 7A 13 13 70 70 7F 7E 8F 8B 9F 78 8E 70 78 7F | 7E 0F 9F 0E 9F 79 31 31 30 0F 3F 31 8F 30 3E 30`.
- Panel colours go through `CPanel = 6 7 8 9 10 32..40` and the owning window's palette; that chain is not
  resolved yet (to do before the `dn` theme's panel colours are taken from here rather than FAR's `dn_like`).

## Norton Commander 5.51
No sources. Taken from the program itself run in DOSBox 0.74 (2026-11-08, 80x25, default colours; captures and
the per-cell attribute dump `cells.py` were in the session scratchpad).
- Panels: text 1B, column headers 1E, frame double 1B, cursor 30; the active panel's path title 30 on the frame,
  the passive one 1B. No clock.
- Menu bar 30, hotkey 3E, open item 0F (hotkey 0E). Pull-down: single frame 30, text 3F, hotkey 3E, separator
  `├─┤`, selected 0F (hotkey 0E), check mark `√`.
- Key bar: number 07, label 30. Command line 07.
- Dialog: background 7, text 7F, a one-cell margin and then a double frame 7F; the title sits on the frame as
  ` Copy ` in F7 (light gray on white). Separators are single lines 7F joined to the double frame (`╟─╢`).
- Input line 0F (white on black), the brackets around it in 7F.
- Check box `[ ]` / `[x]`, text 7F, hotkey 7E.
- Buttons: ` Copy ` in F7 with the hotkey F0 (black); the focused button E4 (red on yellow). Shadow as in
  Turbo Vision: `▄` right of the label row, `▀` below from column 1, black on the dialog background (70).
- Window shadow (dialogs, pull-downs): 2 columns right, 1 row down; the characters stay, attribute 07.

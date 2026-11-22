# ADR-0008: Themes: palette, colour roles and the look of elements in one switchable file

- Date: 2026-11-08
- Status: accepted (owner, 2026-11-08)
- Replaces: the fixed colour functions of `ui/theme.rs` and box characters written where elements are drawn

## Context
Colours are fixed in code (`ui/theme.rs`, one function per FAR palette entry); frame characters, button brackets
and shadows are written where each element is drawn. The owner wants FAR's colours as the default, but also a
compact, understandable way to change colours and the look of elements (dialogs, buttons, controls), kept and
switched as themes (Norton Commander 5, DOS Navigator), working on other operating systems later.

How others do it:
- FAR (`palette.cpp`, `setcolor.cpp`, `Addons/Colors/*.farconfig`): about 150 flat entries
  (`Dialog.Button.Highlight.Selected`), each with its own colour, XML import/export; colours only, the look of
  elements is fixed in code. A theme means going through all 150 entries.
- Midnight Commander skins: one INI file in sections (`[core]`, `[dialog]`, `[menu]`, `[buttonbar]`); `_default_`
  in a section colours everything not given; `[Lines]` gives the frame characters. A skin can be 20 lines.
- Total / Double Commander: a dozen colours in the options plus dark/light mode; simple, little control.
- helix, yazi, zellij: a palette of named colours, roles referring to it, and `inherits` so a theme only gives what
  differs from its base.

## Decision
A theme is one JSON file (the settings are JSON too; no new dependency) in three parts, plus inheritance.

```json
{
  "name": "Norton Commander 5",
  "inherits": "far",
  "palette": { "panelBg": "blue", "accent": "cyan", "text": "lightcyan" },
  "colors": {
    "panel": "text on panelBg",
    "panel.cursor": "black on accent",
    "panel.selected": "yellow on panelBg",
    "dialog": "black on lightgray",
    "dialog.button.focused": "black on accent",
    "keybar.key": "lightgray on black",
    "keybar.label": "black on accent"
  },
  "look": {
    "dialogFrame": "double",
    "menuFrame": "single",
    "buttons": "shadow"
  }
}
```

1. **Palette**: named colours. A colour is one of the 16 standard names (`black` ... `white`, FAR's names),
   `0`-`255`, or `#rrggbb`; a palette name can be used wherever a colour is.
2. **Colours by role**: `"<fg> on <bg>"` (either side may be left out: `"yellow"`, `"on blue"`). Roles are dotted
   (`area.element.state`); a role not given falls back to its parent: `dialog.button.focused` -> `dialog.button`
   -> `dialog`. A full theme needs about 30-40 roles; an inheriting theme a handful of lines. In code the roles are
   an enum (the compiler checks every use); the dotted names are their file names.
3. **Look of elements** (what FAR cannot change): frame sets (`double`, `single`, `rounded`, `ascii`) for panels,
   dialogs and menus; buttons (`far`: `[ OK ]` with `{ OK }` for the default one; `shadow`: a button with a shadow
   under it as in Turbo Vision, NC5, DN); check box and radio marks; window shadows; scroll bar characters. Each
   is a closed set of values, so every theme draws with the same code. Done so far (2026-11-08): `dialogFrame`,
   `menuFrame` (`double` | `single` | `rounded` | `ascii`), `buttons` (`brackets` | `shadow`) and window shadows in
   the `shadow` colour for every theme (FAR's `DropShadow`, now under dialogs too). Panels keep the double frame:
   FAR, NC5 and DN all draw it. Check boxes, radio buttons and scroll bars get their keys when the program has
   them. The `nc5` colours and look are taken from Norton Commander 5.51 itself (`docs/themes-reference.md`).
4. **Inheritance**: `inherits` names a built-in theme or another theme file; what is not given comes from there.
   Built-in themes are compiled in: `far` (the default, FAR's palette exactly), `nc5`, `dn` (grey Turbo Vision,
   buttons with shadows), `mono`.
5. **Where**: theme files in a `themes` folder next to the settings (the portable installation, ADR-0003); on
   Linux/macOS also `~/.config/nova-commander/themes`. Only `src/platform` knows these paths; the theme engine
   does not depend on the operating system.
6. **Colour depth**: themes are written in 16/256/RGB colours, never in console attributes. When the terminal
   cannot show RGB (the legacy Windows console, some Linux terminals) a colour is drawn as the nearest of 256 or 16
   colours; what the terminal can do is found once at start.
7. **In the program only the choice of a ready theme** (owner, 2026-11-08): Options > Themes lists the built-in
   and file themes; moving over one shows the program in it at once, Esc goes back, Enter keeps it (settings key
   `screen.theme`). Making and changing themes (colours by role, the look of elements, a sample of each) is a
   separate utility of the package, made later with its own ADR; it writes theme files the program reads. The
   program has no colour dialogs. Amended 2026-11-11 (owner): FAR's Options > Files highlighting and sort groups
   (the groups' menu, each group's conditions, colours and mark) is in that utility too; the program reads the
   groups (`files.highlighting`), draws by them and sorts by the sort groups, and has no editor of them.
8. Amended 2026-11-09: the frame is also drawn in the theme's exact colours (`#rrggbb` for the 16 names), because a
   Windows Terminal tab drops the table set with `OSC 4` whenever the keyboard layout changes.
   **The console's own colours** (owner, 2026-11-08): `consolePalette` gives the 16 colours by name as `#rrggbb`, and
   the program sets them in the console (`OSC 4`, as FAR's `Classic palette`), giving the console its colours back on
   exit (read at the start and sent back with `OSC 4`, the classic console ignoring `OSC 104`; then `OSC 104` for Windows Terminal, whose scheme cannot be read). The built-in themes carry their references (`docs/themes-reference.md`); `inherits` applies; a
   theme without one leaves the console's. `screen.consolePalette` turns it off.
9. **Later**: import of FAR `.farconfig` colours (known FAR names mapped to roles); file highlighting colours
   (FAR's highlighting groups) as theme roles.

## Consequences
- Steps: (1) one `Theme` value drawn everywhere instead of fixed colour functions, the built-in `far` theme:
  nothing visible changes; (2) theme files, inheritance, Options > Themes, `nc5` / `dn` / `mono` colours (FAR's
  own schemes); (3) the look of elements (frames, shadowed buttons, window shadows) for `nc5` and `dn`; (4) colour
  depth; (5) the theme utility (own ADR).
- `ui` gets a theme module that parses and resolves themes (pure, testable); `app` keeps the chosen theme and
  Options > Themes; `platform` gives the theme folders. No new dependency (serde_json is there).
- Theme files are data, not code: an unknown role or value is reported once and ignored, never a crash.
- FAR's own colour files are not read in step 1-4; a FAR user's scheme needs the later import.

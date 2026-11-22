# ADR-0026: the editor's plugins, API 10

- Date: 2026-11-19
- Status: accepted (roadmap step S2d.7: FAR's editor plugins editcase, brackets, align, autowrap,
  drawline; ADR-0016's rule: any change of the ABI is a new version, every manifest "api": 10)

## Context
FAR's editor plugins work through `EditorControl` (`ECTL_*`, `far/editor.cpp` `Editor::EditorControl`) from
their `OpenW` with `OPEN_EDITOR`, chosen in the editor's F11 menu (plugins flagged `PF_EDITOR`), and some
through `ProcessEditorInputW` (AutoWrap) or a key loop of their own (`ECTL_READINPUT`, DrawLine). The program's
plugins had no way to reach the editor. The editor's view knows lines by their bytes (ADR-0015), plugins by
their numbers.

## Decision
- **S2d.7a, API 10**: `PLACE_EDITOR` (6), the items of the editor's F11 "Plugin commands" (the program's own
  menu, as F11's of the panels: sorted, hotkeys by text, F3 / F4); one chosen runs with `run_item` while the
  editor is up and opens no panel. `NcHost::editor(action, answer)` (appended after `transfer`) does FAR's
  `EditorControl` for the commands the five plugins use: `EDITOR_INFO` (`ECTL_GETINFO`), `GET_LINE`,
  `SET_LINE`, `INSERT_LINE`, `DELETE_LINE`, `INSERT_TEXT`, `DELETE_CHAR`, `SET_POSITION`, `SELECT`,
  `UNDO_REDO`, `REDRAW`, `EXPAND_TABS`, `REAL_TO_TAB`, `TAB_TO_REAL`, `FILE_NAME`, each as FAR's code does it
  (a line's block part as `GetRealSelection` / the column block's columns, `SETSTRING` keeping the cursor's
  character position, `SELECT`'s stream block ending at the start position and the width on its last line,
  an edit refused in lock mode). Lines by number from 0 (-1 the cursor's), positions in characters.
- The request goes to the program as `Event::EditorAsk` and is answered on its thread: a read waits until
  the editor's steps asked before are done and the blocks it needs are read (`EditorText::query`), a change
  goes in as the editor's steps in order (`Step::SetLineText`, `PluginText`, `Break`, `Place`,
  `SelectRange`, `Group`, ...), so a plugin's edits and the keys typed meanwhile keep their order. A line's
  number is found by the view's line counter and walked to from the cursor's line or the document's first.
- The SDK: `Plugin::editor_items` / `run_editor_item`; `Host::editor_info`, `editor_line`,
  `set_editor_line`, `editor_insert_line`, `editor_delete_line`, `editor_insert_text`, `editor_delete_char`,
  `set_editor_position`, `editor_select`, `editor_undo`, `editor_redraw`, `editor_expand_tabs`,
  `editor_real_to_tab`, `editor_tab_to_real`, `editor_file_name`.
- API 10 is amended within S2d.7 while it is not released (as API 9 was): the editor's key hook for AutoWrap
  (S2d.7c) and the key loop for DrawLine (S2d.7d) come there.
- **S2d.7c, the key hook** (FAR's `ProcessEditorInputW`): a plugin with an item in `PLACE_EDITOR_KEYS` (7,
  any label; the SDK's `Plugin::takes_editor_keys`) hears each key the editor has done by its name through
  `NcPlugin::editor_key` (appended after `process_host_file`; the SDK's `Plugin::editor_key`). The editor does
  the key first and the plugin is called after it, the keys typed meanwhile held as during any plugin's call
  (ADR-0019's wait), so its edits come before the next key's: AutoWrap's own `ECTL_PROCESSINPUT` runs the key
  first as well. FAR lets the plugin eat the key; no plugin of the five needs that, so the hook only hears it.
- **S2d.7d, the key loop** (DrawLine's): `EDITOR_READ_INPUT` (`ECTL_READINPUT`) answers with the next key
  pressed in the editor by its name, waiting for it - the keys held while the plugin worked first, then
  each key as it comes, given to the loop before anything else while nothing is over the editor; the call's
  "working" window is not shown while it waits for the user, and its time starts anew with each key.
  `EDITOR_PROCESS_INPUT` (`ECTL_PROCESSINPUT`) does a key by its name as the editor's own (the editor's
  screen keys too); FAR's also tells it to the plugins' key hooks first - here they do not hear it (the
  loop's call holds the keys; AutoWrap is the only hook). `EDITOR_SET_TITLE` (`ECTL_SETTITLE`) puts a text
  in the status line instead of the file's name, `EDITOR_SET_KEY_BAR` (`ECTL_SETKEYBAR`) changes the labels
  of the function keys it names (`KeyBar::Change`), empty restoring the editor's own. Mouse records are not
  read: they stay the editor's.

## Consequences
- Every plugin's manifest says "api": 10; the probe has editor items for the tests.
- A plugin's command on a document too big to count its lines walks them (FAR reads the whole file anyway).

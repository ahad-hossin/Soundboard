# Folders — Design Spec

Date: 2026-11-16
Status: Approved by user, awaiting implementation plan

## Goal

Let the user group sounds into folders. Folders show as tiles in the sound grid;
clicking one opens it, a Back tile returns to the top level. The phone remote
mirrors the same structure. Several sounds can be selected at once and moved to
a folder in one step.

## Scope

### In

- One level of folders. A sound is either on the top level or in exactly one folder.
- Create, rename, delete folders. Deleting a folder moves its sounds to the top level;
  no sound is ever deleted by deleting a folder.
- Move sounds into or out of folders: drag onto a folder tile or the Back tile,
  right-click "Move to folder…", or multi-select then move.
- Multi-select on the desktop (Ctrl+click, Shift+click, Ctrl+A).
- Phone remote shows folder tiles, opens them, Back tile.

### Out

- Nested folders.
- Per-folder color or image.
- Tags (a sound in several folders).
- Multi-select on the phone remote.
- Bulk actions other than move (bulk delete, bulk volume).
- Remembering which folder was open across restarts.
- Reordering folders by drag. Folders keep creation order.

## Data model (`core/src/config.rs`)

```rust
pub struct Folder {
    pub id: String,   // same id scheme as sounds
    pub name: String,
}

pub struct Config {
    // ...existing fields...
    #[serde(default)]
    pub folders: Vec<Folder>,
}

pub struct Sound {
    // ...existing fields...
    /// Id of the folder this sound is in; None means the top level.
    #[serde(default)]
    pub folder: Option<String>,
}
```

- Both fields default to empty, so configs from 0.1.2 load unchanged. `CONFIG_VERSION`
  stays 1 (the change is purely additive).
- Order: `config.sounds` keeps one global order. A view (top level or a folder) shows the
  sounds whose `folder` matches, in that global order. Folders show in `config.folders` order.
- `sanitize`: a sound whose `folder` names no existing folder gets `folder = None`.
  Folder names are trimmed; an empty name becomes "Folder".
- Helper functions on `Config`, unit tested in core:
  - `create_folder(name) -> String` (returns the new id)
  - `rename_folder(id, name) -> Result`
  - `delete_folder(id) -> Result` (sounds in it get `folder = None`)
  - `move_sounds(ids, folder: Option<&str>) -> Result` (unknown folder is an error;
    unknown sound ids are skipped)
- `adopt_orphans` is unchanged: adopted sounds land on the top level.

## Tauri commands (`src-tauri/src/commands.rs`)

New, each using `mutate` so the change is saved immediately and state is broadcast:

- `create_folder(name) -> String`
- `rename_folder(id, name)`
- `delete_folder(id)`
- `move_sounds(ids: Vec<String>, folder: Option<String>)`: one call, one save, for any
  number of sounds.

Changed:

- `add_sounds(paths, folder: Option<String>)`: imported sounds get that folder. An unknown
  folder id falls back to the top level instead of failing the import.

`UiState` gains `folders: Vec<UiFolder { id, name }>`; `UiSound` gains `folder: Option<String>`.
`reorder_sounds` is unchanged: the UI reorders within a view by moving the dragged id to
the target's position in the global id list, which keeps every other view's relative order.

## Desktop UI

### Store (`src/lib/store.ts`)

- `openFolder: string | null` (null = top level). Not persisted.
- `selection: Set<string>` plus `anchor: string | null` for Shift ranges.
- Opening or leaving a folder clears the selection.
- Pure selection logic lives in `src/lib/selection.ts` (toggle, range from anchor over the
  current view's ordered ids, select all) so it is unit tested without React.

### Grid (`src/components/TileGrid.tsx`)

Top level, in order: folder tiles, sound tiles of the top level, `+` tile, dashed
"New folder" tile.

Inside a folder: Back tile, that folder's sound tiles, `+` tile.

- **Folder tile** (`FolderTile.tsx`): monochrome surface, folder name, sound count, and a
  2×2 preview of the colors of its first four sounds. Shows the amber ring when any of its
  sounds is playing. Click or Enter opens it. Right-click: Rename, Delete (confirm dialog
  says the sounds move to the top level).
- **Back tile**: arrow and "Back". Click, Enter or Backspace (when not typing) returns to
  the top level.
- **New folder tile**: opens the existing `RenameDialog` titled "New folder".
- Empty-board state is unchanged when there are no sounds and no folders.

### Selecting

- Ctrl+click toggles a sound in the selection. Shift+click selects the range from the
  anchor to this tile within the current view. Plain click plays, as today.
- Ctrl+A (when not typing) selects every sound in the current view.
- Selected tiles show a white outline and a check mark in a corner. Amber stays reserved
  for playing.
- Esc: if the selection is non-empty, clear it; otherwise Stop All (current behavior).

### Moving

- **Selection bar**: shown above the grid while the selection is non-empty:
  `N selected · Move to folder… · Clear`.
- **Move to folder… picker**: popover listing each folder, "Top level", and "New folder…".
  The current location is shown disabled. "New folder…" asks for a name, creates the folder,
  then moves the sounds into it.
- **Context menu** on a sound gains "Move to folder…". On a selected sound with more than
  one selected it reads "Move N sounds to folder…" and acts on the whole selection.
- **Drag**: dropping a sound on a folder tile moves it in; dropping on the Back tile moves it
  to the top level. If the dragged sound is part of the selection, the whole selection moves.
  Dropping on another sound reorders, as today.
- After any move the selection clears.

### Adding

`+`, Add, and drag-drop of files pass `openFolder` to `add_sounds`, so new sounds land in
the open folder.

### Unchanged

Global hotkeys and Stop All work regardless of the open folder. Trim, rename, color, image,
hotkey and delete per sound work the same inside folders.

## Phone remote

### Protocol (`core/src/remote/protocol.rs`)

```rust
pub struct RemoteFolder { pub id: String, pub name: String }

pub struct RemoteSound { /* existing */ pub folder: Option<String> }

ServerMsg::State { master_volume, playing, sounds, folders: Vec<RemoteFolder> }
```

`src-tauri/src/state.rs` fills both from the config.

### Page (`core/src/remote/page.html`)

- Top level: folder tiles, then top-level sounds. Folder tile looks like the desktop one
  (name, count, color preview, amber ring while any sound inside plays).
- Tap a folder to open it; first tile is Back. Open folder is kept in page memory only.
- If the open folder disappears (deleted on the desktop), the page returns to the top level.
- Tap to play, long-press volume, Stop All and master volume are unchanged.

## Error handling

- Commands on unknown folder ids return an error string, shown as a toast (existing `run`).
- A move to a folder deleted a moment earlier fails with a toast; the selection is kept so
  the user can retry.
- Folder name: trimmed, max 60 characters; empty becomes "Folder". Duplicate names are allowed.

## Testing

Core (`cargo test -p soundboard-core`):
- Old config without `folders` / `folder` loads with an empty folder list and all sounds on
  the top level.
- Sanitize clears dangling `folder` references.
- create / rename / delete folder; delete moves its sounds to the top level.
- `move_sounds` with several ids, with `None`, with an unknown folder (error), with unknown
  sound ids (skipped).
- `ServerMsg::State` serialization includes `folders` and each sound's `folder`.

UI (`npm test`):
- `selection.ts`: toggle, Shift range in both directions, select all, range across a view only.
- Grid: folder tiles render first at the top level with count; opening a folder shows Back
  and only its sounds; Back returns.
- Esc clears a non-empty selection without calling Stop All; with nothing selected it calls
  Stop All.
- Selection bar appears with the count; Move to folder… calls `moveSounds` once with all ids.
- Context menu on a selected sound reads "Move N sounds to folder…".
- Add while a folder is open passes that folder id.

Manual check in the running app: drag a selection onto a folder tile and onto Back, delete a
non-empty folder, phone page folder navigation.

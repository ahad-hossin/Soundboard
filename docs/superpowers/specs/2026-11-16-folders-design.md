# Folders — Design Spec

Date: 2026-11-16
Status: Approved by user (lean version), awaiting implementation

## Goal

Let the user group sounds into folders. Folders show as tiles in the sound grid;
clicking one opens it, a Back tile returns to the top level. The phone remote
mirrors the same structure. Several sounds can be selected with Ctrl+click and
moved to a folder in one step.

## Scope

### In

- One level of folders. A sound is either on the top level or in exactly one folder.
- Create, rename, delete folders. Deleting a folder moves its sounds to the top level;
  no sound is ever deleted by deleting a folder.
- Move sounds with right-click "Move to folder…" or by selecting several with Ctrl+click
  and using the selection bar.
- Phone remote shows folder tiles, opens them, Back tile.

### Out (cut on purpose, lean version)

- Dragging sounds onto a folder or Back tile (drag still reorders within a view).
- Shift+click ranges, Ctrl+A, Backspace to leave a folder.
- Creating a folder from inside the move picker.
- Nested folders, per-folder color or image, tags.
- Multi-select on the phone remote; bulk actions other than move.
- Remembering the open folder across restarts; reordering folders.

## Data model (`core/src/config.rs`)

```rust
pub struct Folder { pub id: String, pub name: String }

pub struct Config { /* existing */ #[serde(default)] pub folders: Vec<Folder> }

pub struct Sound { /* existing */ #[serde(default)] pub folder: Option<String> }
```

- Both fields default to empty, so configs from 0.1.2 load unchanged. `CONFIG_VERSION` stays 1.
- Order: `config.sounds` keeps one global order. A view (top level or a folder) shows the
  sounds whose `folder` matches, in that order. Folders show in `config.folders` order.
- `sanitize`: a sound whose `folder` names no existing folder gets `folder = None`.
- Helpers on `Config`, unit tested: `folder`, `create_folder(name) -> id`,
  `rename_folder`, `delete_folder` (its sounds get `folder = None`),
  `move_sounds(ids, Option<folder>)` (unknown folder is an error and changes nothing;
  unknown sound ids are skipped).
- Folder names: trimmed, max 60 characters, empty becomes "Folder". Duplicates allowed.

## Tauri commands

New, each saves immediately and broadcasts state: `create_folder(name) -> id`,
`rename_folder(id, name)`, `delete_folder(id)`, `move_sounds(ids, folder)`.
Changed: `add_sounds(paths, folder)`; an unknown folder falls back to the top level.
`UiState.folders`, `UiSound.folder`. `reorder_sounds` is unchanged.

## Desktop UI

- Store: `openFolder: string | null` (not persisted) and `sel` (selected sound ids).
  Opening or leaving a folder, moving, and deleting a sound clear the selection.
- Top level grid: folder tiles, top-level sounds, `+`, dashed "New folder" tile.
  Inside a folder: Back tile, that folder's sounds, `+`.
- Folder tile: monochrome, name, sound count, 2×2 preview of its first four sounds' colors,
  amber ring while any of its sounds plays. Click opens. Right-click: Rename, Delete
  (confirm says sounds move to the top level).
- Ctrl+click toggles a sound in the selection; plain click plays. Selected tiles: white
  outline and a check mark.
- Selection bar above the grid while anything is selected: `N selected · Move to folder… · Clear`.
- "Move to folder…" (bar, or a sound's context menu; on a selected sound it reads
  "Move N sounds to folder…" and acts on the whole selection) opens the existing context
  menu with "Top level" and each folder, minus the current location. Hidden when there is
  nowhere to move.
- Esc: clears a non-empty selection; otherwise Stop All.
- `+`, Add, and file drag-drop import into the open folder.
- Hotkeys and Stop All are global and unaffected.

## Phone remote

- `ServerMsg::State` gains `folders: [RemoteFolder { id, name }]`; `RemoteSound` gains `folder`.
- Page: folder tiles then top-level sounds; tap a folder to open it, Back tile first inside.
  Open folder kept in page memory; a folder that disappears returns the page to the top level.

## Error handling

- Commands on unknown folder ids return "That folder no longer exists", shown as a toast.
- A failed move keeps the selection so the user can retry.

## Testing

- Core: old config loads with no folders; sanitize clears dangling folder refs; folder
  create/rename/delete; delete keeps sounds; `move_sounds` (many, None, unknown folder,
  unknown ids); round trip; protocol serialization.
- UI: selection toggle and target ids; view filter and folder summary; Esc order; add into
  open folder; move clears selection, failure keeps it; grid order at top level and inside a
  folder; Ctrl+click selects without playing; selection bar move calls one `moveSounds`;
  context menu label on a selected sound; delete folder confirm; New folder tile.
- Manual: real app and phone page.

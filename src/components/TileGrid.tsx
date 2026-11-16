import { closestCenter, DndContext, PointerSensor, useSensor, useSensors, type DragEndEvent } from "@dnd-kit/core";
import { arrayMove, rectSortingStrategy, SortableContext } from "@dnd-kit/sortable";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useState, type MouseEvent } from "react";
import { addPaths, moveTo, pickAndAddSounds, pickImageFor } from "../lib/actions";
import { api, type UiFolder, type UiSound } from "../lib/api";
import { cx } from "../lib/cx";
import { targetIds, toggle } from "../lib/selection";
import { useStore } from "../lib/store";
import { summarize, viewSounds } from "../lib/view";
import { Banner } from "./Banner";
import { BassPopover } from "./BassPopover";
import { ColorPicker } from "./ColorPicker";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { ConfirmDialog, RenameDialog } from "./Dialogs";
import { EmptyState } from "./EmptyState";
import { BackTile, FolderTile, NewFolderTile } from "./FolderTiles";
import { HotkeyCapture } from "./HotkeyCapture";
import { SelectionBar } from "./SelectionBar";
import { Tile } from "./Tile";
import { TrimEditor } from "./TrimEditor";

type Menu = { sound: UiSound; x: number; y: number };
type FolderMenu = { folder: UiFolder; x: number; y: number };
type Moving = { ids: string[]; x: number; y: number };

export function TileGrid() {
  const state = useStore((s) => s.state)!;
  const play = useStore((s) => s.play);
  const run = useStore((s) => s.run);
  const optimistic = useStore((s) => s.optimistic);
  const pendingTrimId = useStore((s) => s.pendingTrimId);
  const setPendingTrim = useStore((s) => s.setPendingTrim);
  const openFolder = useStore((s) => s.openFolder);
  const setOpenFolder = useStore((s) => s.setOpenFolder);
  const sel = useStore((s) => s.sel);
  const setSel = useStore((s) => s.setSel);
  const clearSel = useStore((s) => s.clearSel);

  const [menu, setMenu] = useState<Menu | null>(null);
  const [deleting, setDeleting] = useState<UiSound | null>(null);
  const [renaming, setRenaming] = useState<UiSound | null>(null);
  const [picking, setPicking] = useState<Menu | null>(null);
  const [capturing, setCapturing] = useState<UiSound | null>(null);
  const [trimming, setTrimming] = useState<UiSound | null>(null);
  const [bassing, setBassing] = useState<Menu | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [folderMenu, setFolderMenu] = useState<FolderMenu | null>(null);
  const [renamingFolder, setRenamingFolder] = useState<UiFolder | null>(null);
  const [deletingFolder, setDeletingFolder] = useState<UiFolder | null>(null);
  const [creatingFolder, setCreatingFolder] = useState(false);
  const [moving, setMoving] = useState<Moving | null>(null);

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "enter" || e.payload.type === "over") setDragOver(true);
      else if (e.payload.type === "leave") setDragOver(false);
      else if (e.payload.type === "drop") {
        setDragOver(false);
        void addPaths(e.payload.paths);
      }
    });
    return () => { void unlisten.then((f) => f()); };
  }, []);

  const allIds = state.sounds.map((s) => s.id);
  const view = viewSounds(state.sounds, openFolder);
  // Places a move can go: top level and every folder, minus where the sounds are now.
  const destinations = [{ id: null, name: "Top level" }, ...state.folders].filter((f) => f.id !== openFolder);

  const onDragEnd = (ev: DragEndEvent) => {
    const { active, over } = ev;
    if (!over || active.id === over.id) return;
    void run(api.reorderSounds(arrayMove(allIds, allIds.indexOf(String(active.id)), allIds.indexOf(String(over.id)))));
  };

  const menuItems = (s: UiSound): MenuItem[] => {
    const ids = targetIds(sel, s.id, allIds);
    return [
      ...(destinations.length > 0
        ? [{ label: ids.length > 1 ? `Move ${ids.length} sounds to folder…` : "Move to folder…", onSelect: () => menu && setMoving({ ids, x: menu.x, y: menu.y }) }]
        : []),
      { label: "Rename", onSelect: () => setRenaming(s) },
      { label: s.trim ? "Trim audio (trimmed)" : "Trim audio", onSelect: () => setTrimming(s) },
      { label: s.bass > 0 ? `Bass boost (${Math.round(s.bass * 100)}%)` : "Bass boost…", onSelect: () => menu && setBassing({ sound: s, x: menu.x, y: menu.y }) },
      { label: s.image_path ? "Change image" : "Set image", onSelect: () => void pickImageFor(s.id) },
      ...(s.image_path ? [{ label: "Remove image", onSelect: () => void run(api.clearSoundImage(s.id)) }] : []),
      { label: "Change color", onSelect: () => menu && setPicking({ sound: s, x: menu.x, y: menu.y }) },
      { label: s.hotkey ? "Change hotkey" : "Set hotkey", onSelect: () => setCapturing(s) },
      ...(s.hotkey ? [{ label: "Clear hotkey", onSelect: () => void run(api.clearHotkey(s.id)) }] : []),
      { label: "Delete", danger: true, onSelect: () => setDeleting(s) },
    ];
  };

  const folderItems = (f: UiFolder): MenuItem[] => [
    { label: "Rename", onSelect: () => setRenamingFolder(f) },
    { label: "Delete", danger: true, onSelect: () => setDeletingFolder(f) },
  ];

  const moveItems = (m: Moving): MenuItem[] => destinations.map((f) => ({ label: f.name, onSelect: () => void moveTo(m.ids, f.id) }));

  const openMoveFromBar = (e: MouseEvent<HTMLButtonElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    setMoving({ ids: allIds.filter((id) => sel.has(id)), x: r.left, y: r.bottom + 4 });
  };

  const noDevices = !state.devices.speakers && !state.devices.virtual_mic;

  if (state.sounds.length === 0 && state.folders.length === 0) {
    return (
      <div className="flex h-full flex-col">
        {noDevices && <Banner />}
        <div className="min-h-0 flex-1">
          <EmptyState dragOver={dragOver} />
        </div>
      </div>
    );
  }

  const playingSet = new Set([...state.playing, ...optimistic]);
  // A single import opens the editor once the new sound is in state.
  const imported = pendingTrimId ? state.sounds.find((s) => s.id === pendingTrimId) : undefined;

  return (
    <div className={cx("min-h-full rounded-md border border-transparent", dragOver && "border-accent")}>
      {noDevices && <Banner />}
      {sel.size > 0 && <SelectionBar count={sel.size} onMove={destinations.length > 0 ? openMoveFromBar : undefined} onClear={clearSel} />}
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd}>
        <SortableContext items={view.map((s) => s.id)} strategy={rectSortingStrategy}>
          <div className="grid gap-2" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(120px, 1fr))" }}>
            {openFolder === null
              ? state.folders.map((f) => (
                  <FolderTile
                    key={f.id}
                    folder={f}
                    summary={summarize(state.sounds, f.id, playingSet)}
                    onOpen={() => setOpenFolder(f.id)}
                    onContextMenu={(e) => { e.preventDefault(); setFolderMenu({ folder: f, x: e.clientX, y: e.clientY }); }}
                  />
                ))
              : <BackTile onBack={() => setOpenFolder(null)} />}
            {view.map((s) => (
              <Tile
                key={s.id}
                sound={s}
                playing={playingSet.has(s.id)}
                selected={sel.has(s.id)}
                onToggleSelect={() => setSel(toggle(sel, s.id))}
                onPlay={() => void play(s.id)}
                onVolume={(v) => void run(api.setSoundVolume(s.id, v))}
                onContextMenu={(e) => { e.preventDefault(); setMenu({ sound: s, x: e.clientX, y: e.clientY }); }}
              />
            ))}
            <button
              type="button"
              onClick={() => void pickAndAddSounds()}
              aria-label="Add sounds"
              title="Add sounds"
              className="flex aspect-square items-center justify-center rounded-md border border-dashed border-border text-3xl font-light text-mute outline-none ring-accent transition-colors hover:border-dim hover:text-dim focus-visible:ring-2"
            >
              +
            </button>
            {openFolder === null && <NewFolderTile onClick={() => setCreatingFolder(true)} />}
          </div>
        </SortableContext>
      </DndContext>

      {menu && <ContextMenu x={menu.x} y={menu.y} items={menuItems(menu.sound)} onClose={() => setMenu(null)} />}
      {folderMenu && <ContextMenu x={folderMenu.x} y={folderMenu.y} items={folderItems(folderMenu.folder)} onClose={() => setFolderMenu(null)} />}
      {moving && <ContextMenu x={moving.x} y={moving.y} items={moveItems(moving)} onClose={() => setMoving(null)} />}
      {creatingFolder && (
        <RenameDialog
          title="New folder"
          initial=""
          onCancel={() => setCreatingFolder(false)}
          onSubmit={(name) => { void run(api.createFolder(name)); setCreatingFolder(false); }}
        />
      )}
      {renamingFolder && (
        <RenameDialog
          title="Folder name"
          initial={renamingFolder.name}
          onCancel={() => setRenamingFolder(null)}
          onSubmit={(name) => { void run(api.renameFolder(renamingFolder.id, name)); setRenamingFolder(null); }}
        />
      )}
      {deletingFolder && (
        <ConfirmDialog
          title={`Delete folder "${deletingFolder.name}"? Sounds inside move to the top level.`}
          confirmLabel="Delete"
          danger
          onCancel={() => setDeletingFolder(null)}
          onConfirm={() => { void run(api.deleteFolder(deletingFolder.id)); setDeletingFolder(null); }}
        />
      )}
      {bassing && (
        <BassPopover
          x={bassing.x}
          y={bassing.y}
          name={bassing.sound.name}
          value={bassing.sound.bass}
          onCommit={(v) => void run(api.setBass(bassing.sound.id, v))}
          onClose={() => setBassing(null)}
        />
      )}
      {picking && (
        <ColorPicker
          x={picking.x}
          y={picking.y}
          current={picking.sound.color}
          onPick={(color) => void run(api.setSoundColor(picking.sound.id, color))}
          onClose={() => setPicking(null)}
        />
      )}
      {deleting && (
        <ConfirmDialog
          title={`Delete "${deleting.name}"? The file is removed from the soundboard.`}
          confirmLabel="Delete"
          danger
          onCancel={() => setDeleting(null)}
          onConfirm={() => { void run(api.deleteSound(deleting.id)); clearSel(); setDeleting(null); }}
        />
      )}
      {renaming && (
        <RenameDialog
          title="Name"
          initial={renaming.name}
          onCancel={() => setRenaming(null)}
          onSubmit={(name) => { void run(api.renameSound(renaming.id, name)); setRenaming(null); }}
        />
      )}
      {capturing && (
        <HotkeyCapture
          title={`Hotkey for "${capturing.name}"`}
          current={capturing.hotkey}
          onCancel={() => setCapturing(null)}
          onCapture={(accel) => { void run(api.setHotkey(capturing.id, accel)); setCapturing(null); }}
          onClear={capturing.hotkey ? () => { void run(api.clearHotkey(capturing.id)); setCapturing(null); } : undefined}
        />
      )}
      {trimming && <TrimEditor key={trimming.id} sound={trimming} mode="edit" onClose={() => setTrimming(null)} />}
      {imported && !trimming && <TrimEditor key={imported.id} sound={imported} mode="import" onClose={() => setPendingTrim(null)} />}
    </div>
  );
}

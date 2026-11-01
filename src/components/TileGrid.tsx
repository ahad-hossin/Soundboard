import { closestCenter, DndContext, PointerSensor, useSensor, useSensors, type DragEndEvent } from "@dnd-kit/core";
import { arrayMove, rectSortingStrategy, SortableContext } from "@dnd-kit/sortable";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useState } from "react";
import { addPaths, pickAndAddSounds, pickImageFor } from "../lib/actions";
import { api, type UiSound } from "../lib/api";
import { cx } from "../lib/cx";
import { useStore } from "../lib/store";
import { Banner } from "./Banner";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { ConfirmDialog, RenameDialog } from "./Dialogs";
import { EmptyState } from "./EmptyState";
import { HotkeyCapture } from "./HotkeyCapture";
import { Tile } from "./Tile";

type Menu = { sound: UiSound; x: number; y: number };

export function TileGrid() {
  const state = useStore((s) => s.state)!;
  const play = useStore((s) => s.play);
  const run = useStore((s) => s.run);
  const optimistic = useStore((s) => s.optimistic);

  const [menu, setMenu] = useState<Menu | null>(null);
  const [deleting, setDeleting] = useState<UiSound | null>(null);
  const [renaming, setRenaming] = useState<UiSound | null>(null);
  const [capturing, setCapturing] = useState<UiSound | null>(null);
  const [dragOver, setDragOver] = useState(false);

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

  const onDragEnd = (ev: DragEndEvent) => {
    const { active, over } = ev;
    if (!over || active.id === over.id) return;
    const ids = state.sounds.map((s) => s.id);
    void run(api.reorderSounds(arrayMove(ids, ids.indexOf(String(active.id)), ids.indexOf(String(over.id)))));
  };

  const menuItems = (s: UiSound): MenuItem[] => [
    { label: "Rename", onSelect: () => setRenaming(s) },
    { label: s.image_path ? "Change image" : "Set image", onSelect: () => void pickImageFor(s.id) },
    ...(s.image_path ? [{ label: "Remove image", onSelect: () => void run(api.clearSoundImage(s.id)) }] : []),
    { label: s.hotkey ? "Change hotkey" : "Set hotkey", onSelect: () => setCapturing(s) },
    ...(s.hotkey ? [{ label: "Clear hotkey", onSelect: () => void run(api.clearHotkey(s.id)) }] : []),
    { label: "Delete", danger: true, onSelect: () => setDeleting(s) },
  ];

  const noDevices = !state.devices.speakers && !state.devices.virtual_mic;

  if (state.sounds.length === 0) {
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

  return (
    <div className={cx("min-h-full rounded-md border border-transparent", dragOver && "border-accent")}>
      {noDevices && <Banner />}
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd}>
        <SortableContext items={state.sounds.map((s) => s.id)} strategy={rectSortingStrategy}>
          <div className="grid gap-2" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(120px, 1fr))" }}>
            {state.sounds.map((s) => (
              <Tile
                key={s.id}
                sound={s}
                playing={playingSet.has(s.id)}
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
          </div>
        </SortableContext>
      </DndContext>

      {menu && <ContextMenu x={menu.x} y={menu.y} items={menuItems(menu.sound)} onClose={() => setMenu(null)} />}
      {deleting && (
        <ConfirmDialog
          title={`Delete "${deleting.name}"? The file is removed from the soundboard.`}
          confirmLabel="Delete"
          danger
          onCancel={() => setDeleting(null)}
          onConfirm={() => { void run(api.deleteSound(deleting.id)); setDeleting(null); }}
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
    </div>
  );
}

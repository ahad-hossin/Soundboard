import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import { useStore } from "./store";

const AUDIO = ["mp3", "wav", "ogg", "flac"];
const IMAGES = ["png", "jpg", "jpeg", "webp", "gif"];

/** Imports files; each rejected file becomes a warning toast. A single new sound opens the trim editor. */
export async function addPaths(paths: string[]) {
  const { toast, setPendingTrim } = useStore.getState();
  try {
    const { rejected, added_ids } = await api.addSounds(paths);
    rejected.forEach((r) => toast("warn", r));
    if (added_ids.length === 1) setPendingTrim(added_ids[0]);
  } catch (e) {
    toast("error", String(e));
  }
}

export async function pickAndAddSounds() {
  const picked = await open({ multiple: true, filters: [{ name: "Audio", extensions: AUDIO }] });
  if (!picked) return;
  await addPaths(Array.isArray(picked) ? picked : [picked]);
}

export async function pickImageFor(id: string) {
  const picked = await open({ multiple: false, filters: [{ name: "Image", extensions: IMAGES }] });
  if (!picked || Array.isArray(picked)) return;
  await useStore.getState().run(api.setSoundImage(id, picked));
}

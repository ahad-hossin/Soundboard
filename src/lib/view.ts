import type { UiSound } from "./api";

/** Sounds shown in a view: the top level (`folder` null) or one folder, in the global order. */
export function viewSounds(sounds: readonly UiSound[], folder: string | null): UiSound[] {
  return sounds.filter((s) => s.folder === folder);
}

export interface FolderSummary { count: number; colors: string[]; playing: boolean }

/** What a folder tile shows: how many sounds, the colors of the first four, and whether any is playing. */
export function summarize(sounds: readonly UiSound[], folderId: string, playing: ReadonlySet<string>): FolderSummary {
  const inside = viewSounds(sounds, folderId);
  return { count: inside.length, colors: inside.slice(0, 4).map((s) => s.color), playing: inside.some((s) => playing.has(s.id)) };
}

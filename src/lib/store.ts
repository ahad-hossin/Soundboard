import { create } from "zustand";
import { api, type Toast, type ToastLevel, type UiState } from "./api";

export type Panel = "settings" | "remote" | null;
export interface ToastItem extends Toast { id: number }

interface Store {
  state: UiState | null;
  panel: Panel;
  toasts: ToastItem[];
  /** Sound ids shown as playing before Rust confirms. */
  optimistic: Set<string>;
  /** A sound just imported on its own; the grid opens the trim editor for it once it is in state. */
  pendingTrimId: string | null;
  setPendingTrim: (id: string | null) => void;
  init: () => Promise<void>;
  setPanel: (p: Panel) => void;
  togglePanel: (p: Exclude<Panel, null>) => void;
  toast: (level: ToastLevel, message: string) => void;
  dismissToast: (id: number) => void;
  play: (id: string) => Promise<void>;
  /** Awaits a command; a rejection becomes an error toast. */
  run: (p: Promise<unknown>) => Promise<void>;
}

let nextToastId = 1;
let started = false;

export const useStore = create<Store>((set, get) => ({
  state: null,
  panel: null,
  toasts: [],
  optimistic: new Set(),
  pendingTrimId: null,
  setPendingTrim: (pendingTrimId) => set({ pendingTrimId }),

  init: async () => {
    if (started) return;
    started = true;
    await api.onState((s) => set({ state: s }));
    await api.onToast((t) => get().toast(t.level, t.message));
    const s = await api.getState();
    set({ state: s });
    s.startup_notices.forEach((n) => get().toast("warn", n));
  },

  setPanel: (panel) => set({ panel }),
  togglePanel: (p) => set((st) => ({ panel: st.panel === p ? null : p })),

  toast: (level, message) => {
    const id = nextToastId++;
    set((st) => ({ toasts: [...st.toasts, { id, level, message }].slice(-3) }));
    setTimeout(() => get().dismissToast(id), 4000);
  },
  dismissToast: (id) => set((st) => ({ toasts: st.toasts.filter((t) => t.id !== id) })),

  play: async (id) => {
    set((st) => ({ optimistic: new Set(st.optimistic).add(id) }));
    try {
      await api.play(id);
    } catch (e) {
      get().toast("error", String(e));
    } finally {
      setTimeout(() => {
        set((st) => {
          const n = new Set(st.optimistic);
          n.delete(id);
          return { optimistic: n };
        });
      }, 150);
    }
  },

  run: async (p) => {
    try {
      await p;
    } catch (e) {
      get().toast("error", String(e));
    }
  },
}));

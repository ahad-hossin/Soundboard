import { api } from "./api";
import { isTypingTarget } from "./keys";
import { useStore } from "./store";

/** Window-level keys. Dialogs and popovers catch their own Esc first (capture phase). */
export function handleGlobalKey(e: Pick<KeyboardEvent, "key" | "target">) {
  if (e.key !== "Escape" || isTypingTarget(e.target)) return;
  const { sel, clearSel } = useStore.getState();
  if (sel.size > 0) clearSel();
  else void api.stopAll();
}

/** Desktop multi-select: the selected sound ids. */
export const NO_SELECTION: ReadonlySet<string> = new Set();

/** Ctrl+click: add or remove one sound. Returns a new set. */
export function toggle(sel: ReadonlySet<string>, id: string): Set<string> {
  const next = new Set(sel);
  if (!next.delete(id)) next.add(id);
  return next;
}

/** The sounds an action on `id` applies to: the whole selection (in `orderedIds` order) when `id` is in it, else just `id`. */
export function targetIds(sel: ReadonlySet<string>, id: string, orderedIds: readonly string[]): string[] {
  return sel.has(id) ? orderedIds.filter((x) => sel.has(x)) : [id];
}

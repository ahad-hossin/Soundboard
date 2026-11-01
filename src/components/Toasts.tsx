import { cx } from "../lib/cx";
import { useStore } from "../lib/store";

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  const dismiss = useStore((s) => s.dismissToast);
  if (toasts.length === 0) return null;
  return (
    <div className="pointer-events-none fixed bottom-3 left-3 flex w-80 flex-col gap-2" role="status" aria-live="polite">
      {toasts.map((t) => (
        <button
          key={t.id}
          type="button"
          onClick={() => dismiss(t.id)}
          className={cx(
            "pointer-events-auto rounded-md border-l-2 bg-surface px-3 py-2 text-left shadow-lg",
            t.level === "error" && "border-danger",
            t.level === "warn" && "border-dim",
            t.level === "info" && "border-dim",
          )}
        >
          {t.message}
        </button>
      ))}
    </div>
  );
}

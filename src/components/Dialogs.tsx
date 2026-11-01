import { useEffect, useRef, useState, type ReactNode } from "react";
import { cx } from "../lib/cx";

function Backdrop({ children, onCancel }: { children: ReactNode; onCancel: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopPropagation(); onCancel(); } };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onCancel]);
  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/50" onMouseDown={onCancel}>
      <div role="dialog" className="w-80 rounded-md border border-border bg-surface p-4 shadow-xl" onMouseDown={(e) => e.stopPropagation()}>
        {children}
      </div>
    </div>
  );
}

interface ConfirmProps { title: string; confirmLabel: string; danger?: boolean; onConfirm: () => void; onCancel: () => void }

export function ConfirmDialog({ title, confirmLabel, danger, onConfirm, onCancel }: ConfirmProps) {
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => ref.current?.focus(), []);
  return (
    <Backdrop onCancel={onCancel}>
      <p className="mb-4">{title}</p>
      <div className="flex justify-end gap-2">
        <button type="button" className="btn" onClick={onCancel}>Cancel</button>
        <button ref={ref} type="button" onClick={onConfirm} className={cx("btn", danger && "border-danger bg-danger text-white hover:bg-danger")}>
          {confirmLabel}
        </button>
      </div>
    </Backdrop>
  );
}

interface RenameProps { title: string; initial: string; onSubmit: (value: string) => void; onCancel: () => void }

export function RenameDialog({ title, initial, onSubmit, onCancel }: RenameProps) {
  const [value, setValue] = useState(initial);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => { ref.current?.focus(); ref.current?.select(); }, []);
  const submit = () => { const v = value.trim(); if (v) onSubmit(v); };
  return (
    <Backdrop onCancel={onCancel}>
      <form onSubmit={(e) => { e.preventDefault(); submit(); }}>
        <label className="label" htmlFor="rename">{title}</label>
        <input id="rename" ref={ref} className="field mb-4" value={value} onChange={(e) => setValue(e.target.value)} />
        <div className="flex justify-end gap-2">
          <button type="button" className="btn" onClick={onCancel}>Cancel</button>
          <button type="submit" className="btn border-text bg-text text-bg hover:bg-text" disabled={!value.trim()}>Save</button>
        </div>
      </form>
    </Backdrop>
  );
}

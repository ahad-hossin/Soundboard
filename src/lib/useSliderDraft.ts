import { useEffect, useRef, useState, type ChangeEvent } from "react";

/**
 * Range-input props that keep the value local while the user drags and only
 * call `commit` when the gesture ends (pointer up, key up, blur). Follows the
 * prop again whenever no drag is in progress.
 */
export function useSliderDraft(value: number, commit: (v: number) => void) {
  const [draft, setDraft] = useState(value);
  const dirty = useRef(false);
  const latest = useRef(value);

  useEffect(() => {
    if (!dirty.current) {
      latest.current = value;
      setDraft(value);
    }
  }, [value]);

  const finish = () => {
    if (!dirty.current) return;
    dirty.current = false;
    commit(latest.current);
  };

  return {
    value: draft,
    onChange: (e: ChangeEvent<HTMLInputElement>) => {
      const v = Number(e.target.value);
      dirty.current = true;
      latest.current = v;
      setDraft(v);
    },
    onPointerUp: finish,
    onKeyUp: finish,
    onBlur: finish,
  };
}

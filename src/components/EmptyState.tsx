import { pickAndAddSounds } from "../lib/actions";
import { cx } from "../lib/cx";

export function EmptyState({ dragOver }: { dragOver: boolean }) {
  return (
    <div className={cx("flex h-full items-center justify-center rounded-md border border-dashed text-dim", dragOver ? "border-accent" : "border-border")}>
      <span>
        Drop audio files here, or{" "}
        <button type="button" className="underline underline-offset-2 hover:text-text" onClick={() => void pickAndAddSounds()}>
          Add
        </button>
        .
      </span>
    </div>
  );
}

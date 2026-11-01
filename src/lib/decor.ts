import type { CSSProperties } from "react";

/** Number of motifs; keep in sync with the phone page (`core/src/remote/page.html`). */
const MOTIFS = 4;

/** Stable motif for a sound: sum of char codes mod 4. */
export function motifIndex(id: string): number {
  let sum = 0;
  for (let i = 0; i < id.length; i++) sum += id.charCodeAt(i);
  return sum % MOTIFS;
}

const HIGHLIGHT = "radial-gradient(circle at 25% 20%, rgba(255,255,255,0.22), transparent 60%)";

// Each motif is one background layer: [image, size, position, repeat]. Circle and ring are
// radial gradients in an oversized box (closest-side) whose negative percent position puts
// the box where the brief asks (tiles are square, so width percentages work on both axes).
const MOTIF_LAYERS: ReadonlyArray<readonly [string, string, string, string]> = [
  // Circle: 110% disc centered at 85% / 90%.
  ["radial-gradient(circle closest-side, rgba(255,255,255,0.12) 99%, transparent 100%)", "110% 110%", "-300% -350%", "no-repeat"],
  // Ring: 4 px ring, 70% wide, centered at 80% / 25%.
  [
    "radial-gradient(circle closest-side, transparent calc(100% - 4px), rgba(255,255,255,0.13) calc(100% - 3.5px), rgba(255,255,255,0.13) calc(100% - 0.5px), transparent 100%)",
    "70% 70%",
    "150% -33.333%",
    "no-repeat",
  ],
  // Band: diagonal stripe.
  ["linear-gradient(135deg, transparent 40%, rgba(255,255,255,0.13) 40%, rgba(255,255,255,0.13) 58%, transparent 58%)", "100% 100%", "0 0", "no-repeat"],
  // Dots: a 14 px grid.
  ["radial-gradient(rgba(255,255,255,0.14) 1.5px, transparent 1.6px)", "14px 14px", "0 0", "repeat"],
];

/** Background layers for a tile without an image: highlight and one motif over the color fill. */
export function tileDecoration(id: string, color: string): CSSProperties {
  const [image, size, position, repeat] = MOTIF_LAYERS[motifIndex(id)];
  return {
    backgroundImage: `${HIGHLIGHT}, ${image}`,
    backgroundSize: `100% 100%, ${size}`,
    backgroundPosition: `0 0, ${position}`,
    backgroundRepeat: `no-repeat, ${repeat}`,
    backgroundColor: color,
  };
}

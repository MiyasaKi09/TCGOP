// ============================================================
// Pixi-facing adapter over ELEMENT_STYLE (src/lib/vfx.ts).
// Pure data — no pixi import — so it is safe to import anywhere.
// Converts the CSS hex palette into numeric colors + per-element
// particle tuning consumed by the WebGL effects.
// ============================================================

import { ELEMENT_STYLE, type VfxElement } from "../vfx";

/** "#E0653C" | "#abc" -> 0xE0653C */
export function hexToNum(hex: string): number {
  let h = hex.trim().replace("#", "");
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  return parseInt(h.slice(0, 6), 16) || 0xffffff;
}

export type Blend = "add" | "normal";

export interface ElementVfxConfig {
  key: VfxElement;
  color: number;
  blend: Blend;
  /** particles emitted per ~16ms along the projectile path */
  trailRate: number;
  /** particles in the impact burst */
  burstCount: number;
  /** px/s² downward pull on particles (embers rise = negative) */
  gravity: number;
  /** particle lifetime ms */
  life: number;
  /** base particle size px */
  size: number;
  /** do particles spin (ice/sand) */
  spin: boolean;
}

// Per-element feel. Phase 1 uses a shared generic look; these knobs let
// Phase 2 differentiate without touching the engine.
const TUNING: Record<VfxElement, Partial<ElementVfxConfig>> = {
  fire:     { blend: "add", trailRate: 3, burstCount: 26, gravity: -40, life: 620, size: 16 },
  water:    { blend: "normal", trailRate: 2, burstCount: 22, gravity: 220, life: 560, size: 14 },
  thunder:  { blend: "add", trailRate: 2, burstCount: 20, gravity: 0, life: 420, size: 13 },
  ice:      { blend: "add", trailRate: 2, burstCount: 24, gravity: 60, life: 640, size: 14, spin: true },
  sand:     { blend: "normal", trailRate: 3, burstCount: 26, gravity: 30, life: 600, size: 13, spin: true },
  poison:   { blend: "normal", trailRate: 2, burstCount: 20, gravity: -30, life: 700, size: 13 },
  physical: { blend: "add", trailRate: 2, burstCount: 18, gravity: 40, life: 460, size: 13 },
  haki:     { blend: "add", trailRate: 2, burstCount: 24, gravity: 0, life: 560, size: 16 },
};

const DEFAULTS: Omit<ElementVfxConfig, "key" | "color"> = {
  blend: "add", trailRate: 2, burstCount: 20, gravity: 40, life: 560, size: 14, spin: false,
};

export function elementVfx(key: VfxElement): ElementVfxConfig {
  const style = ELEMENT_STYLE[key] ?? ELEMENT_STYLE.physical;
  return { key, color: hexToNum(style.color), ...DEFAULTS, ...TUNING[key] };
}

export const HEAL_NUM = 0x5bc46a;

// ============================================================
// motion — shared easing + duration tokens so every animation in
// the app (FLIP zoom, reveals, menus, CSS) shares one feel.
// Mirrored as CSS custom properties in globals.css (:root).
// ============================================================

/** Smooth, slightly springy settle — menus, FLIP zoom. */
export const EASE_SPRING = "cubic-bezier(.2,.75,.25,1)";
/** Fast out, gentle in — entrances. */
export const EASE_OUT_EXPO = "cubic-bezier(.16,1,.3,1)";
/** Decisive ease — flying cards / projectiles. */
export const EASE_OUT = "cubic-bezier(.4,.1,.3,1)";

export const DUR_FAST = 180;
export const DUR_BASE = 300;
export const DUR_SLOW = 500;
export const DUR_REVEAL = 1150;

/** Build a `transition` value, e.g. tx("transform", DUR_BASE, EASE_SPRING). */
export function tx(prop: string, ms: number, ease: string = EASE_OUT): string {
  return `${prop} ${ms}ms ${ease}`;
}

// ============================================================
// vfxBus — a module-level, fire-and-forget event emitter that
// bridges the React VFX detection (useCombatVfx) to the imperative
// WebGL layer (VfxStage / CutInLayer) WITHOUT going through React
// state. Transient particles must never trigger reconciliation.
// ============================================================

import type { VfxEvent } from "./useCombatVfx";

type Listener = (ev: VfxEvent) => void;

const listeners = new Set<Listener>();

/** Emit a VFX event to every current subscriber. Safe to call on the server (no-op-ish). */
export function emit(ev: VfxEvent): void {
  // Snapshot to tolerate (un)subscribe during dispatch.
  for (const fn of Array.from(listeners)) {
    try { fn(ev); } catch { /* a broken listener must not break the game */ }
  }
}

/** Subscribe to VFX events; returns an unsubscribe function. */
export function subscribe(fn: Listener): () => void {
  listeners.add(fn);
  return () => { listeners.delete(fn); };
}

export const vfxBus = { emit, subscribe };

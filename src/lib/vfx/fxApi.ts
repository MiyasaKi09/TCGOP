// Shared handle passed to every effect spawner. Type-only pixi imports
// (erased at build) so importing this never pulls pixi into a static graph.
import type { Application, Container, Texture } from "pixi.js";
import type { ParticleField } from "./particles";

export interface FxApi {
  app: Application;
  field: ParticleField;
  /** normal-blend mid layer (rings, heads) */
  fx: Container;
  /** additive (glow) layer */
  glow: Container;
  /** soft dot texture */
  dot: Texture;
  /** ring/annulus texture */
  ring: Texture;
  /** register a per-frame updater; return false to stop & remove it */
  addUpdater(fn: (dtMS: number) => boolean): void;
  /** brief radial shockwave ripple centred at x,y */
  shockwave(x: number, y: number, big: boolean): void;
}

export const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
export const rand = (a: number, b: number) => a + Math.random() * (b - a);
export const easeOut = (k: number) => 1 - (1 - k) * (1 - k);

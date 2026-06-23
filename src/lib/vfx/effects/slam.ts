import { Sprite } from "pixi.js";
import type { VfxEvent } from "../../useCombatVfx";
import { rand, type FxApi } from "../fxApi";

const DUST = 0xcdb38a;

/** A unit lands: expanding dust ring + low kicked-up dust particles. */
export function spawnSlam(api: FxApi, ev: VfxEvent): void {
  const x = ev.toX ?? 0, y = ev.toY ?? 0;

  for (let i = 0; i < 16; i++) {
    const a = -Math.PI + rand(-0.5, 0.5) + (i % 2 ? 0 : Math.PI); // mostly sideways
    const sp = rand(80, 220);
    api.field.spawn({
      x: x + rand(-10, 10), y: y + rand(20, 34),
      vx: Math.cos(a) * sp, vy: -rand(20, 70),
      life: rand(360, 560), color: DUST, size: rand(8, 14),
      gravity: 240, spin: 0, blend: "normal", alpha: 0.7,
    });
  }

  const ring = new Sprite(api.ring);
  ring.anchor.set(0.5);
  ring.tint = DUST;
  ring.blendMode = "normal";
  ring.x = x; ring.y = y + 30;
  ring.scale.y = 0.4; // flattened (ground)
  api.fx.addChild(ring);
  let t = 0;
  const dur = 360;
  api.addUpdater((dt) => {
    t += dt;
    const k = Math.min(1, t / dur);
    ring.scale.set(0.2 + 1.8 * k, (0.2 + 1.8 * k) * 0.4);
    ring.alpha = (1 - k) * 0.6;
    if (k >= 1) { ring.destroy(); return false; }
    return true;
  });
}

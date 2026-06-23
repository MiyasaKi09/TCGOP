import { Sprite } from "pixi.js";
import type { VfxEvent } from "../../useCombatVfx";
import { rand, type FxApi } from "../fxApi";

const GOLD = 0xe8b84b;

/** Implosion → bright gold burst + expanding ring when a unit is KO'd. */
export function spawnKO(api: FxApi, ev: VfxEvent): void {
  const x = ev.toX ?? 0, y = ev.toY ?? 0;

  // implosion: particles rush inward
  for (let i = 0; i < 26; i++) {
    const a = Math.random() * Math.PI * 2;
    const r = rand(34, 64);
    api.field.spawn({
      x: x + Math.cos(a) * r, y: y + Math.sin(a) * r,
      vx: -Math.cos(a) * rand(140, 280), vy: -Math.sin(a) * rand(140, 280),
      life: 360, color: 0xffffff, size: 12, gravity: 0, spin: 0, blend: "add",
    });
  }

  // delayed outward burst
  window.setTimeout(() => {
    for (let i = 0; i < 34; i++) {
      const a = Math.random() * Math.PI * 2;
      const sp = rand(120, 440);
      api.field.spawn({
        x, y, vx: Math.cos(a) * sp, vy: Math.sin(a) * sp,
        life: rand(440, 640), color: GOLD, size: 13, gravity: 160, spin: 0, blend: "add",
      });
    }
  }, 190);

  // ring
  const ring = new Sprite(api.ring);
  ring.anchor.set(0.5);
  ring.tint = GOLD;
  ring.blendMode = "add";
  ring.x = x; ring.y = y;
  api.glow.addChild(ring);
  let t = 0;
  const dur = 520;
  api.addUpdater((dt) => {
    t += dt;
    const k = Math.min(1, t / dur);
    ring.scale.set(0.2 + 2.4 * k);
    ring.alpha = (1 - k) * 0.9;
    if (k >= 1) { ring.destroy(); return false; }
    return true;
  });
}

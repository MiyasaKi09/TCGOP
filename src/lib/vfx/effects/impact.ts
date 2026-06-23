import { Sprite } from "pixi.js";
import type { VfxEvent } from "../../useCombatVfx";
import { elementVfx } from "../elements";
import { rand, type FxApi } from "../fxApi";

/** Radial particle burst + an expanding additive ring at the hit point. */
export function spawnImpact(api: FxApi, ev: VfxEvent): void {
  const cfg = elementVfx(ev.element);
  const x = ev.toX ?? 0, y = ev.toY ?? 0;
  const big = !!ev.big;
  if (big) api.shockwave(x, y, true);
  const count = Math.round(cfg.burstCount * (big ? 1.7 : 1));

  for (let i = 0; i < count; i++) {
    const a = Math.random() * Math.PI * 2;
    const speed = rand(70, big ? 460 : 280);
    api.field.spawn({
      x, y, vx: Math.cos(a) * speed, vy: Math.sin(a) * speed,
      life: cfg.life, color: cfg.color, size: cfg.size * (big ? 1.2 : 1),
      gravity: cfg.gravity, spin: cfg.spin ? rand(-0.4, 0.4) : 0, blend: cfg.blend,
    });
  }

  const ring = new Sprite(api.ring);
  ring.anchor.set(0.5);
  ring.tint = cfg.color;
  ring.blendMode = "add";
  ring.x = x; ring.y = y;
  api.glow.addChild(ring);

  const dur = big ? 440 : 300;
  const maxScale = big ? 3.0 : 1.9;
  let t = 0;
  api.addUpdater((dt) => {
    t += dt;
    const k = Math.min(1, t / dur);
    ring.scale.set(0.2 + maxScale * k);
    ring.alpha = (1 - k) * 0.85;
    if (k >= 1) { ring.destroy(); return false; }
    return true;
  });
}

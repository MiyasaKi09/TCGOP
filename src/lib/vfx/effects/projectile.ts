import { Sprite } from "pixi.js";
import type { VfxEvent } from "../../useCombatVfx";
import { elementVfx } from "../elements";
import { lerp, rand, easeOut, type FxApi } from "../fxApi";

/**
 * A glowing head travels attacker → target leaving an element-tinted trail.
 * Decoupled from the impact burst (that is its own bus event), matching the
 * existing CSS layer's behaviour.
 */
export function spawnProjectile(api: FxApi, ev: VfxEvent): void {
  const cfg = elementVfx(ev.element);
  const fromX = ev.fromX ?? 0, fromY = ev.fromY ?? 0;
  const toX = ev.toX ?? 0, toY = ev.toY ?? 0;
  const big = !!ev.big;
  const dur = big ? 700 : 540;

  const head = new Sprite(api.dot);
  head.anchor.set(0.5);
  head.tint = cfg.color;
  head.blendMode = "add";
  head.x = fromX; head.y = fromY;
  const headScale = (big ? 40 : 26) / 32;
  head.scale.set(headScale);
  api.glow.addChild(head);

  let t = 0;
  api.addUpdater((dt) => {
    t += dt;
    const k = Math.min(1, t / dur);
    const e = easeOut(k);
    head.x = lerp(fromX, toX, e);
    head.y = lerp(fromY, toY, e);
    head.scale.set(headScale * (1 + 0.12 * Math.sin(k * Math.PI)));

    for (let i = 0; i < cfg.trailRate; i++) {
      api.field.spawn({
        x: head.x + rand(-6, 6), y: head.y + rand(-6, 6),
        vx: rand(-34, 34), vy: rand(-34, 34),
        life: cfg.life * 0.6, color: cfg.color, size: cfg.size * 0.8,
        gravity: cfg.gravity, spin: cfg.spin ? rand(-0.3, 0.3) : 0, blend: cfg.blend,
      });
    }

    if (k >= 1) { head.destroy(); return false; }
    return true;
  });
}

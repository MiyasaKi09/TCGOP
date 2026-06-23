import { Graphics } from "pixi.js";
import type { VfxEvent } from "../../useCombatVfx";
import { elementVfx } from "../elements";
import { rand, type FxApi } from "../fxApi";

function drawBolt(g: Graphics, x1: number, y1: number, x2: number, y2: number, color: number) {
  g.clear();
  const segs = 9;
  g.moveTo(x1, y1);
  for (let i = 1; i <= segs; i++) {
    const t = i / segs;
    const jx = i < segs ? rand(-26, 26) : 0;
    const jy = i < segs ? rand(-26, 26) : 0;
    g.lineTo(x1 + (x2 - x1) * t + jx, y1 + (y2 - y1) * t + jy);
  }
  g.stroke({ width: 3, color, alpha: 1, cap: "round", join: "round" });
}

/** A jagged additive bolt that flickers and fades — near-instant travel. */
export function spawnLightning(api: FxApi, ev: VfxEvent): void {
  const cfg = elementVfx("thunder");
  const x1 = ev.fromX ?? 0, y1 = ev.fromY ?? 0, x2 = ev.toX ?? 0, y2 = ev.toY ?? 0;
  const g = new Graphics();
  g.blendMode = "add";
  api.glow.addChild(g);

  const dur = ev.big ? 380 : 300;
  let t = 0, flick = 0;
  drawBolt(g, x1, y1, x2, y2, cfg.color);
  api.addUpdater((dt) => {
    t += dt; flick -= dt;
    if (flick <= 0) { drawBolt(g, x1, y1, x2, y2, cfg.color); flick = 45; }
    g.alpha = Math.max(0, 1 - t / dur);
    if (t >= dur) { g.destroy(); return false; }
    return true;
  });
}

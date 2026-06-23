import { Graphics } from "pixi.js";
import type { VfxEvent } from "../../useCombatVfx";
import { elementVfx } from "../elements";
import { rand, type FxApi } from "../fxApi";

/** Manga speed-lines converging on centre + a quick white flash, element-tinted. */
export function spawnCutInFx(api: FxApi, ev: VfxEvent): void {
  const W = api.app.screen.width, H = api.app.screen.height;
  const cx = W / 2, cy = H / 2;
  const R = Math.hypot(W, H) / 2;
  const color = elementVfx(ev.element).color;

  const lines = new Graphics();
  lines.blendMode = "add";
  api.glow.addChild(lines);
  const N = 64;
  const seed = Array.from({ length: N }, () => ({
    a: rand(0, Math.PI * 2), inner: R * rand(0.34, 0.46), w: rand(1, 4),
  }));
  const drawLines = () => {
    lines.clear();
    for (const s of seed) {
      lines.moveTo(cx + Math.cos(s.a) * s.inner, cy + Math.sin(s.a) * s.inner);
      lines.lineTo(cx + Math.cos(s.a) * R * 1.25, cy + Math.sin(s.a) * R * 1.25);
      lines.stroke({ width: s.w, color, alpha: 0.6 });
    }
  };
  drawLines();

  const flash = new Graphics().rect(0, 0, W, H).fill({ color: 0xffffff, alpha: 1 });
  flash.blendMode = "add";
  api.glow.addChild(flash);

  const dur = 620;
  let t = 0;
  api.addUpdater((dt) => {
    t += dt;
    const k = Math.min(1, t / dur);
    // lines: quick in, hold, fade
    lines.alpha = k < 0.12 ? k / 0.12 : Math.max(0, 1 - (k - 0.12) / 0.88);
    // flash: very short
    flash.alpha = Math.max(0, 0.55 * (1 - t / 200));
    if (k >= 1) { lines.destroy(); flash.destroy(); return false; }
    return true;
  });
}

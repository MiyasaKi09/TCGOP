import type { VfxEvent } from "../../useCombatVfx";
import { HEAL_NUM } from "../elements";
import { rand, type FxApi } from "../fxApi";

/** Rising additive green sparkles + a soft upward glow (number stays in DOM). */
export function spawnHeal(api: FxApi, ev: VfxEvent): void {
  const x = ev.toX ?? 0, y = ev.toY ?? 0;
  const n = 20;
  for (let i = 0; i < n; i++) {
    const ang = -Math.PI / 2 + rand(-0.7, 0.7);
    const sp = rand(40, 150);
    api.field.spawn({
      x: x + rand(-22, 22), y: y + rand(-8, 14),
      vx: Math.cos(ang) * sp, vy: Math.sin(ang) * sp,
      life: rand(560, 820), color: HEAL_NUM, size: rand(9, 14),
      gravity: -40, spin: 0, blend: "add",
    });
  }
}

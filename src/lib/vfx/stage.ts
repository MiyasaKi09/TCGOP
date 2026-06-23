// ============================================================
// stage.ts — imperative PixiJS orchestrator. The ONLY module that
// statically imports pixi; it is reached exclusively via dynamic
// import from VfxStage.tsx, so pixi stays in an async chunk and
// never runs during SSR/`next build`.
// ============================================================

import { Application, Container, Graphics, type Filter, type Texture } from "pixi.js";
import { AdvancedBloomFilter, ShockwaveFilter } from "pixi-filters";
import { ParticleField } from "./particles";
import { subscribe } from "../vfxBus";
import type { VfxEvent } from "../useCombatVfx";
import type { FxApi } from "./fxApi";
import { spawnProjectile } from "./effects/projectile";
import { spawnImpact } from "./effects/impact";
import { spawnLightning } from "./effects/lightning";
import { spawnHeal } from "./effects/heal";
import { spawnKO } from "./effects/ko";
import { spawnCutInFx } from "./effects/cutin";
import { spawnSlam } from "./effects/slam";

export interface VfxStageHandle { destroy(): void; }

export interface StageOptions {
  cap?: number;
  onContextLost?: () => void;
}

type Updater = (dtMS: number) => boolean;

export async function createVfxStage(host: HTMLElement, opts: StageOptions = {}): Promise<VfxStageHandle> {
  const app = new Application();
  await app.init({
    resizeTo: window,
    backgroundAlpha: 0,
    antialias: true,
    resolution: Math.min(typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1, 2),
    autoDensity: true,
    powerPreference: "high-performance",
  });

  const canvas = app.canvas as HTMLCanvasElement;
  canvas.style.position = "absolute";
  canvas.style.inset = "0";
  canvas.style.width = "100%";
  canvas.style.height = "100%";
  canvas.style.pointerEvents = "none";
  host.appendChild(canvas);

  // --- shared textures ---
  const dotG = new Graphics();
  for (let i = 6; i >= 1; i--) dotG.circle(0, 0, (i / 6) * 16).fill({ color: 0xffffff, alpha: 0.16 });
  const dot: Texture = app.renderer.generateTexture(dotG);
  dotG.destroy();

  const ringG = new Graphics().circle(0, 0, 22).stroke({ width: 4, color: 0xffffff, alpha: 1 });
  const ring: Texture = app.renderer.generateTexture(ringG);
  ringG.destroy();

  // --- layers (normal blend below, additive glow above) ---
  const fx = new Container();
  const glow = new Container();
  app.stage.addChild(fx, glow);

  // Bloom only on the additive glow layer (cost control); fixed filterArea
  // avoids per-frame bounds recompute.
  glow.filterArea = app.screen;
  glow.filters = [new AdvancedBloomFilter({ threshold: 0.25, bloomScale: 1.15, brightness: 1.0, blur: 6, quality: 4 })];

  const field = new ParticleField(glow, fx, dot, opts.cap ?? 600);

  const updaters: Updater[] = [];
  const shockwaves: ShockwaveFilter[] = [];
  const syncStageFilters = () => { app.stage.filters = (shockwaves.length ? [...shockwaves] : []) as Filter[]; };

  const api: FxApi = {
    app, field, fx, glow, dot, ring,
    addUpdater(fn) { updaters.push(fn); },
    shockwave(x, y, big) {
      const sw = new ShockwaveFilter({ center: { x, y }, amplitude: big ? 18 : 10, wavelength: big ? 170 : 120, brightness: 1.0, speed: 900, radius: -1 });
      shockwaves.push(sw);
      syncStageFilters();
      let t = 0;
      const dur = big ? 600 : 440;
      updaters.push((dt) => {
        t += dt;
        sw.time = t / 1000;
        if (t >= dur) {
          const i = shockwaves.indexOf(sw);
          if (i >= 0) shockwaves.splice(i, 1);
          syncStageFilters();
          return false;
        }
        return true;
      });
    },
  };

  let lost = false;
  app.ticker.add((tk) => {
    if (lost) return;
    const dt = tk.deltaMS;
    field.update(dt);
    for (let i = updaters.length - 1; i >= 0; i--) {
      if (!updaters[i](dt)) updaters.splice(i, 1);
    }
  });

  function dispatch(ev: VfxEvent) {
    if (lost) return;
    switch (ev.kind) {
      case "attack":
        if (ev.big) spawnCutInFx(api, ev);
        if (ev.element === "thunder") spawnLightning(api, ev);
        else spawnProjectile(api, ev);
        break;
      case "impact": spawnImpact(api, ev); break;
      case "heal": spawnHeal(api, ev); break;
      case "ko": spawnKO(api, ev); break;
      case "spawn": spawnSlam(api, ev); break;
      // banner text stays in the DOM layer (crisp/localised)
    }
  }
  const unsub = subscribe(dispatch);

  const onLost = (e: Event) => { e.preventDefault(); lost = true; opts.onContextLost?.(); };
  canvas.addEventListener("webglcontextlost", onLost, false);

  return {
    destroy() {
      unsub();
      canvas.removeEventListener("webglcontextlost", onLost);
      updaters.length = 0;
      field.destroy();
      app.destroy(true, { children: true, texture: true });
    },
  };
}

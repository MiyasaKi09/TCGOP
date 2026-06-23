// ============================================================
// ambient.ts — a low-cost background Pixi app that animates the sea:
// the sea texture rippled by a scrolling blurred-noise DisplacementFilter,
// plus a slow god-ray. Separate app/ticker, quality-gated by the caller.
// Reached only via dynamic import → pixi stays lazy. Fully defensive:
// any failure leaves the static .sea-bg in place.
// ============================================================

import { Application, Assets, Sprite, Texture, DisplacementFilter, Graphics } from "pixi.js";

export interface AmbientHandle { destroy(): void; }

function coverSprite(sp: Sprite, w: number, h: number) {
  const tw = sp.texture.width || 1, th = sp.texture.height || 1;
  const scale = Math.max(w / tw, h / th);
  sp.scale.set(scale);
  sp.x = (w - tw * scale) / 2;
  sp.y = (h - th * scale) / 2;
}

function makeNoiseTexture(size = 256): Texture {
  const c = document.createElement("canvas");
  c.width = c.height = size;
  const ctx = c.getContext("2d")!;
  ctx.fillStyle = "#808080";
  ctx.fillRect(0, 0, size, size);
  ctx.filter = "blur(7px)";
  for (let i = 0; i < 150; i++) {
    const v = Math.floor(Math.random() * 256);
    ctx.fillStyle = `rgb(${v},${v},${v})`;
    ctx.beginPath();
    ctx.arc(Math.random() * size, Math.random() * size, Math.random() * 22 + 8, 0, Math.PI * 2);
    ctx.fill();
  }
  return Texture.from(c);
}

export async function createAmbientStage(host: HTMLElement): Promise<AmbientHandle> {
  const app = new Application();
  await app.init({
    resizeTo: window,
    backgroundAlpha: 0,
    antialias: false,
    resolution: Math.min(typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1, 1.5),
    autoDensity: true,
    powerPreference: "low-power",
  });

  const canvas = app.canvas as HTMLCanvasElement;
  canvas.style.position = "absolute";
  canvas.style.inset = "0";
  canvas.style.width = "100%";
  canvas.style.height = "100%";
  host.appendChild(canvas);

  // --- sea ---
  let seaTex: Texture;
  try { seaTex = await Assets.load("/decks/sea.png"); }
  catch { seaTex = Texture.WHITE; }
  const sea = new Sprite(seaTex);
  const fit = () => coverSprite(sea, app.screen.width, app.screen.height);
  fit();
  app.stage.addChild(sea);

  // --- rippling displacement ---
  const noise = makeNoiseTexture(256);
  try { (noise.source as unknown as { addressMode: string }).addressMode = "repeat"; } catch { /* clamp is fine */ }
  const disp = new Sprite(noise);
  disp.scale.set(2);
  app.stage.addChild(disp);
  const filter = new DisplacementFilter({ sprite: disp, scale: 18 });
  sea.filters = [filter];

  // --- slow god-ray (very low alpha) ---
  const ray = new Graphics()
    .moveTo(0, 0).lineTo(420, 0).lineTo(120, 900).lineTo(-60, 900).closePath()
    .fill({ color: 0xbfe0ff, alpha: 1 });
  ray.alpha = 0.05;
  ray.blendMode = "add";
  ray.x = app.screen.width * 0.5;
  ray.y = -80;
  app.stage.addChild(ray);

  const onResize = () => { fit(); ray.x = app.screen.width * 0.5; };
  window.addEventListener("resize", onResize);

  app.ticker.add((tk) => {
    disp.x += 0.28 * tk.deltaTime;
    disp.y += 0.16 * tk.deltaTime;
    ray.rotation = Math.sin(Date.now() / 9000) * 0.06;
  });

  return {
    destroy() {
      window.removeEventListener("resize", onResize);
      app.destroy(true, { children: true, texture: false });
    },
  };
}

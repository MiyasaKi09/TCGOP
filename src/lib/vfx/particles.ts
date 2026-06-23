// ============================================================
// ParticleField — a small pooled particle system on plain Pixi
// Sprites (one shared soft-dot texture, recycled). One update step
// driven by the stage ticker. Hard cap; excess spawns are dropped.
// Reached only via dynamic import (stage.ts) → keeps pixi lazy.
// ============================================================

import { Container, Sprite, type Texture } from "pixi.js";

const TEX_DIAM = 32; // the base dot texture is 32px across

export interface SpawnOpts {
  x: number; y: number;
  vx: number; vy: number;
  life: number;      // ms
  color: number;
  size: number;      // target diameter px
  gravity: number;   // px/s²
  spin: number;      // rad/frame-ish
  blend: "add" | "normal";
  alpha?: number;
  fade?: boolean;    // shrink + fade over life (default true)
}

interface P {
  sprite: Sprite;
  vx: number; vy: number;
  life: number; maxLife: number;
  spin: number; gravity: number;
  baseScale: number; alpha0: number;
  fade: boolean;
  active: boolean;
}

export class ParticleField {
  private pool: P[] = [];
  private active = 0;

  constructor(
    private addLayer: Container,   // additive blend layer (glow)
    private normalLayer: Container, // normal blend layer
    private tex: Texture,
    private cap: number,
  ) {}

  get count() { return this.active; }

  spawn(o: SpawnOpts): void {
    if (this.active >= this.cap) return;
    const p = this.acquire();
    const sp = p.sprite;
    sp.visible = true;
    sp.tint = o.color;
    sp.blendMode = o.blend === "add" ? "add" : "normal";
    sp.x = o.x; sp.y = o.y;
    sp.rotation = Math.random() * Math.PI * 2;
    p.vx = o.vx; p.vy = o.vy;
    p.gravity = o.gravity;
    p.spin = o.spin;
    p.life = o.life; p.maxLife = o.life;
    p.alpha0 = o.alpha ?? 1;
    p.fade = o.fade !== false;
    p.baseScale = o.size / TEX_DIAM;
    sp.scale.set(p.baseScale);
    sp.alpha = p.alpha0;
    const layer = o.blend === "add" ? this.addLayer : this.normalLayer;
    if (sp.parent !== layer) layer.addChild(sp);
  }

  update(dtMS: number): void {
    const dt = dtMS / 1000;
    for (const p of this.pool) {
      if (!p.active) continue;
      p.life -= dtMS;
      if (p.life <= 0) { this.release(p); continue; }
      p.vy += p.gravity * dt;
      const sp = p.sprite;
      sp.x += p.vx * dt;
      sp.y += p.vy * dt;
      sp.rotation += p.spin;
      const t = p.life / p.maxLife; // 1 → 0
      if (p.fade) {
        sp.alpha = p.alpha0 * t;
        sp.scale.set(p.baseScale * (0.3 + 0.7 * t));
      }
    }
  }

  private acquire(): P {
    for (const p of this.pool) {
      if (!p.active) { p.active = true; this.active++; return p; }
    }
    const sprite = new Sprite(this.tex);
    sprite.anchor.set(0.5);
    const p: P = { sprite, vx: 0, vy: 0, life: 0, maxLife: 1, spin: 0, gravity: 0, baseScale: 1, alpha0: 1, fade: true, active: true };
    this.pool.push(p);
    this.active++;
    return p;
  }

  private release(p: P): void {
    p.active = false;
    p.sprite.visible = false;
    this.active--;
  }

  destroy(): void {
    for (const p of this.pool) p.sprite.destroy();
    this.pool = [];
    this.active = 0;
  }
}

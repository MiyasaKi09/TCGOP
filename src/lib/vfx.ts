// ============================================================
// Combat VFX style library — one coherent, logical look per element
// and per action archetype. Pure data, reused by the VFX layer.
// ============================================================

import type { Element } from "@/types";

export type VfxElement = Element | "physical" | "haki";

export interface ElementStyle {
  key: VfxElement;
  label: string;
  /** Primary colour */
  color: string;
  /** Soft glow colour (rgba) */
  glow: string;
  /** Full-screen tint flash (rgba) */
  tint: string;
  /** Projectile CSS class */
  proj: string;
  /** Impact-burst CSS class */
  burst: string;
  /** Glyph shown on the projectile / burst */
  glyph: string;
}

export const ELEMENT_STYLE: Record<VfxElement, ElementStyle> = {
  fire: {
    key: "fire", label: "Feu", color: "#E0653C", glow: "rgba(224,101,60,.75)",
    tint: "rgba(224,101,60,.16)", proj: "vfx-proj-fire", burst: "vfx-burst-fire", glyph: "🔥",
  },
  water: {
    key: "water", label: "Eau", color: "#3C9FE0", glow: "rgba(60,159,224,.7)",
    tint: "rgba(60,159,224,.16)", proj: "vfx-proj-water", burst: "vfx-burst-water", glyph: "💧",
  },
  thunder: {
    key: "thunder", label: "Foudre", color: "#F2D04B", glow: "rgba(242,208,75,.8)",
    tint: "rgba(242,208,75,.18)", proj: "vfx-proj-thunder", burst: "vfx-burst-thunder", glyph: "⚡",
  },
  ice: {
    key: "ice", label: "Glace", color: "#7FE0F0", glow: "rgba(127,224,240,.7)",
    tint: "rgba(127,224,240,.16)", proj: "vfx-proj-ice", burst: "vfx-burst-ice", glyph: "❄",
  },
  sand: {
    key: "sand", label: "Sable", color: "#D8A85A", glow: "rgba(216,168,90,.7)",
    tint: "rgba(216,168,90,.16)", proj: "vfx-proj-sand", burst: "vfx-burst-sand", glyph: "🌪",
  },
  poison: {
    key: "poison", label: "Poison", color: "#9FCB4A", glow: "rgba(159,203,74,.7)",
    tint: "rgba(159,203,74,.15)", proj: "vfx-proj-poison", burst: "vfx-burst-poison", glyph: "☠",
  },
  physical: {
    key: "physical", label: "", color: "#F0D27A", glow: "rgba(240,210,122,.6)",
    tint: "rgba(240,210,122,.08)", proj: "vfx-proj-physical", burst: "vfx-burst-physical", glyph: "✦",
  },
  haki: {
    key: "haki", label: "Haki", color: "#B57BE6", glow: "rgba(181,123,230,.75)",
    tint: "rgba(30,8,48,.22)", proj: "vfx-proj-haki", burst: "vfx-burst-haki", glyph: "✦",
  },
};

/** Resolve the look for an attack: explicit element wins, else Haki, else physical. */
export function styleFor(element?: Element | null, hasHaki?: boolean): ElementStyle {
  if (element && ELEMENT_STYLE[element]) return ELEMENT_STYLE[element];
  if (hasHaki) return ELEMENT_STYLE.haki;
  return ELEMENT_STYLE.physical;
}

/** Map an event card's effect to an element look (for AoE flair). */
export function eventElement(defId: string): VfxElement {
  switch (defId) {
    case "BW-023": return "sand";    // Tempête de Sable
    case "MR-020": return "fire";    // Buster Call
    case "MG-025": return "physical"; // Tempête
    default: return "physical";
  }
}

export const HEAL_COLOR = "#5BC46A";

// ============================================================
// theme — SINGLE SOURCE OF TRUTH for every runtime visual token.
// Mirrors the `@theme` block + CSS custom properties in globals.css
// so JS/inline styles and CSS/Tailwind utilities share one palette.
//
// Direction artistique : « manga ink » — encre & papier, contours
// marqués, accents de faction punchy, sémantique de stats lisible.
//
// Rule of thumb: components must NOT hardcode hex values. Import from
// here (JS/inline) or use the generated utilities / CSS vars (className).
// ============================================================

// --- Ink & paper surfaces -----------------------------------
export const INK = {
  900: "#05070b", // page background (deepest)
  800: "#0b0e13", // card/token background
  700: "#10141c", // raised panel
  600: "#161c26", // panel highlight
  500: "#1f2733", // hover / active surface
} as const;

/** Off-white "paper" highlight used for manga screentone & light text. */
export const PAPER = "#f3ecdd";

// --- Brand gold ---------------------------------------------
export const GOLD = "#E8B84B";
export const GOLD_DEEP = "#D9A434";

// --- Faction accents ----------------------------------------
export const PIRATE = "#E8954A";
export const MARINE = "#5B97D8";

// --- Stat semantics (atk / def / hp) ------------------------
export const ATK = "#FF7062";
export const DEF = "#7FB0E8";
export const HP_FULL = "#5BC46A";
export const HP_MID = "#E8C53B";
export const HP_LOW = "#FF7062";
export const HEAL = "#5BC46A";

// --- Selection / alert states -------------------------------
export const TARGET = "#E0463F"; // attack target (red)
export const DEPLOY = "#5BC46A"; // valid deploy slot (green)
export const SELECT = GOLD; // selected (gold)
export const IMPACT = "#E8954A"; // AoE/zone splash (orange)

/** Current-PV colour ramp (green → amber → red). */
export function hpColor(ratio: number): string {
  return ratio > 0.5 ? HP_FULL : ratio > 0.25 ? HP_MID : HP_LOW;
}

// --- Faction visual identity --------------------------------
export interface FactionVisual {
  /** Bright accent (labels, edges). */
  accent: string;
  /** Default border colour for tokens without a rarity override. */
  border: string;
  /** Backdrop gradient for frame tokens (no illustration). */
  frame: string;
  /** Soft ambiance wash for a player's half / ship. */
  ambiance: string;
  /** Crest glyph. */
  crest: "hat" | "anchor";
  /** Top-down vertical deck art (bow up). */
  shipImg: string;
  /** Display label. */
  label: string;
}

const PIRATE_FACTION: FactionVisual = {
  accent: PIRATE,
  border: "rgba(232,149,74,.6)",
  frame: "radial-gradient(120% 80% at 50% 6%, #3a2614 0%, #20140b 60%, #140c07 100%)",
  ambiance: "radial-gradient(120% 100% at 50% 100%, rgba(232,149,74,.18), transparent 70%)",
  crest: "hat",
  shipImg: "/decks/mugiwara-deck-v.png",
  label: "Mugiwara",
};

const MARINE_FACTION: FactionVisual = {
  accent: MARINE,
  border: "rgba(62,120,184,.6)",
  frame: "radial-gradient(120% 80% at 50% 6%, #163049 0%, #0c1a29 60%, #08121c 100%)",
  ambiance: "radial-gradient(120% 100% at 50% 0%, rgba(62,120,184,.18), transparent 70%)",
  crest: "anchor",
  shipImg: "/decks/marine-deck-v.png",
  label: "Marine",
};

const FACTION_BY_KEY: Record<string, FactionVisual> = {
  pirate: PIRATE_FACTION,
  marine: MARINE_FACTION,
  revolutionary: PIRATE_FACTION,
  independent: MARINE_FACTION,
};

export function faction(key: string): FactionVisual {
  return FACTION_BY_KEY[key] ?? PIRATE_FACTION;
}

// --- Rarity & trait colours ---------------------------------
/** Rarity → border colour for tokens / cards. */
export const RARITY_BORDER: Record<string, string> = {
  C: "rgba(165,170,180,.55)",
  U: "rgba(74,168,107,.6)",
  R: "rgba(59,130,196,.6)",
  SR: "rgba(168,95,208,.65)",
  L: "rgba(232,184,75,.8)",
  CAP: "rgba(232,184,75,.85)",
};

/** Trait → dot/pill colour (board tokens & detail). */
export const TRAIT_COLOR: Record<string, string> = {
  shield: "#C2925A",
  range: "#E0883C",
  stealth: "#5B8FE0",
  rush: "#D08A3C",
  cursed: "#A86FD0",
  logia: "#6E9AC0",
  piercing: "#6FAE6A",
  conqueror: "#E8B84B",
};

// --- Status effect colours (shared with StatusBadges) -------
export const STATUS_COLOR: Record<string, string> = {
  burn: "#E0653C",
  poison: "#8FB84A",
  freeze: "#5CC6E0",
  desiccation: "#C2925A",
  trap: "#E0463F",
  immobilize: "#E879B0",
  sleep: "#9B8CE0",
  loseAction: "#C9A82E",
  selfKO: "#E0463F",
  noStealth: "#7FB0E8",
  noHeal: "#FF8A80",
};

// --- Element colours (mirror of vfx.ts ELEMENT_STYLE) -------
// Used by card detail pills; the VFX layer keeps its richer style map.
export const ELEMENT_COLOR: Record<string, string> = {
  fire: "#E0653C",
  water: "#3C9FE0",
  thunder: "#C9A82E",
  ice: "#5CC6E0",
  sand: "#C2925A",
  poison: "#8FB84A",
};

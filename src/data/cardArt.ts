// ============================================================
// Visual theme for the "Navires" redesign — art, factions, traits.
// Pure data; consumed by Card / CaptainCard / BoardSlot / Game.
// ============================================================

export const GOLD = "#E8B84B";

/** defId → full-illustration art (the 6 Mugiwara). Everything else uses a frame. */
export const CARD_ART: Record<string, string> = {
  "ST01-001": "/cards/zoro.jpg",
  "ST01-002": "/cards/sanji.jpg",
  "ST01-003": "/cards/nami.jpg",
  "ST01-004": "/cards/usopp.jpg",
  "ST01-005": "/cards/chopper.jpg",
  "ST01-006": "/cards/robin.jpg",
};

/** Focal point used when the art is cropped into a board token. */
export const CARD_ART_FOCUS: Record<string, string> = {
  "ST01-001": "50% 30%",
  "ST01-002": "46% 40%",
  "ST01-003": "50% 30%",
  "ST01-004": "60% 42%",
  "ST01-005": "50% 24%",
  "ST01-006": "58% 34%",
};

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

const PIRATE: FactionVisual = {
  accent: "#E8954A",
  border: "rgba(232,149,74,.6)",
  frame: "radial-gradient(120% 80% at 50% 6%, #3a2614 0%, #20140b 60%, #140c07 100%)",
  ambiance: "radial-gradient(120% 100% at 50% 100%, rgba(232,149,74,.18), transparent 70%)",
  crest: "hat",
  shipImg: "/decks/mugiwara-deck-v.png",
  label: "Mugiwara",
};

const MARINE: FactionVisual = {
  accent: "#5B97D8",
  border: "rgba(62,120,184,.6)",
  frame: "radial-gradient(120% 80% at 50% 6%, #163049 0%, #0c1a29 60%, #08121c 100%)",
  ambiance: "radial-gradient(120% 100% at 50% 0%, rgba(62,120,184,.18), transparent 70%)",
  crest: "anchor",
  shipImg: "/decks/marine-deck-v.png",
  label: "Marine",
};

const FACTION: Record<string, FactionVisual> = {
  pirate: PIRATE,
  marine: MARINE,
  revolutionary: PIRATE,
  independent: MARINE,
};

export function faction(key: string): FactionVisual {
  return FACTION[key] ?? PIRATE;
}

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

export const TRAIT_LABEL: Record<string, string> = {
  shield: "Bouclier",
  range: "Portée",
  stealth: "Furtif",
  rush: "Rush",
  cursed: "Maudit",
  logia: "Logia",
  piercing: "Perçant",
  conqueror: "Conquérant",
};

/** Rarity → border colour for tokens / cards. */
export const RARITY_BORDER: Record<string, string> = {
  C: "rgba(165,170,180,.55)",
  U: "rgba(74,168,107,.6)",
  R: "rgba(59,130,196,.6)",
  SR: "rgba(168,95,208,.65)",
  L: "rgba(232,184,75,.8)",
  CAP: "rgba(232,184,75,.85)",
};

/** Current-PV colour ramp (green → amber → red). */
export function hpColor(ratio: number): string {
  return ratio > 0.5 ? "#5BC46A" : ratio > 0.25 ? "#E8C53B" : "#FF7062";
}

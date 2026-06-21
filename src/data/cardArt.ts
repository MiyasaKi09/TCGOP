// ============================================================
// Visual theme for the "Navires" redesign — art, factions, traits.
// Pure data; consumed by Card / CaptainCard / BoardSlot / Game.
// ============================================================

export const GOLD = "#E8B84B";

/** defId → recto (default) full-illustration art. */
export const CARD_ART: Record<string, string> = {
  // --- Mugiwara characters ---
  "MG-001": "/cards/zoro.jpg",
  "MG-002": "/cards/sanji.jpg",
  "MG-003": "/cards/nami.jpg",
  "MG-004": "/cards/usopp.jpg",
  "MG-005": "/cards/chopper.jpg",
  "MG-006": "/cards/robin.jpg",
  "MG-007": "/cards/franky.jpg",
  "MG-008": "/cards/brook.jpg",
  // --- Weapons ---
  "MG-009": "/cards/wado.jpg",
  "MG-010": "/cards/sandai.jpg",
  "MG-011": "/cards/yubashiri.jpg",
  "MG-012": "/cards/clima-tact.jpg",
  "MG-013": "/cards/kabuto.jpg",
  // --- Devil Fruits (recto face) ---
  "MG-014": "/cards/gomu-recto.jpg",
  "MG-015": "/cards/hana-recto.jpg",
  "MG-016": "/cards/yomi-recto.jpg",
  // --- Accessories ---
  "MG-017": "/cards/baril.jpg",
  "MG-018": "/cards/dial.jpg",
  "MG-019": "/cards/vivre.jpg",
  // --- Ships ---
  "MG-020": "/cards/going-merry.jpg",
  "MG-021": "/cards/thousand-sunny.jpg",
  // --- Events ---
  "MG-022": "/cards/volonte-d.jpg",
  "MG-023": "/cards/nakama.jpg",
  "MG-024": "/cards/promesse.jpg",
  "MG-025": "/cards/tempete.jpg",
  "MG-028": "/cards/coup-de-burst.jpg",
  // --- Counters ---
  "MG-026": "/cards/je-veux-vivre.jpg",
  "MG-027": "/cards/drapeau-noir.jpg",
  // --- Captain Luffy (recto) ---
  "CAP-LUFFY": "/cards/luffy-recto.jpg",
  // --- Marines (ST02) ---
  "MR-001": "/cards/coby.jpg",
  "MR-002": "/cards/helmeppo.jpg",
  "MR-003": "/cards/tashigi.jpg",
  "MR-004": "/cards/smoker.jpg",
  "MR-005": "/cards/sentomaru.jpg",
  "MR-006": "/cards/momonga.jpg",
  "MR-007": "/cards/garp.jpg",
  "MR-008": "/cards/kizaru.jpg",
  "MR-009": "/cards/aokiji.jpg",
  "MR-010": "/cards/sengoku.jpg",
  // --- Captain Akainu (recto) ---
  "CAP-AKAINU": "/cards/akainu-recto.jpg",
};

/** defId → verso/awakened art. Used when a fruit is awakened or a captain is flipped. */
export const CARD_ART_VERSO: Record<string, string> = {
  "MG-014": "/cards/gomu-verso.jpg",
  "MG-015": "/cards/hana-verso.jpg",
  "MG-016": "/cards/yomi-verso.jpg",
  "CAP-LUFFY": "/cards/luffy-verso.jpg",
  "CAP-AKAINU": "/cards/akainu-verso.jpg",
};

/** Focal point used when the art is cropped into a board token / card frame. */
export const CARD_ART_FOCUS: Record<string, string> = {
  "MG-001": "50% 22%",
  "MG-002": "50% 16%",
  "MG-003": "50% 30%",
  "MG-004": "60% 42%",
  "MG-005": "50% 30%",
  "MG-006": "55% 30%",
  "MG-007": "50% 26%",
  "MG-008": "50% 16%",
  "CAP-LUFFY": "50% 24%",
  "CAP-AKAINU": "50% 24%",
  "MR-001": "50% 22%",
  "MR-002": "50% 22%",
  "MR-003": "50% 22%",
  "MR-004": "50% 22%",
  "MR-005": "50% 26%",
  "MR-006": "50% 28%",
  "MR-007": "50% 24%",
  "MR-008": "50% 26%",
  "MR-009": "50% 24%",
  "MR-010": "50% 26%",
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

/** Flavour quotes shown vertically next to the name. */
export const QUOTE: Record<string, string> = {
  "MG-001": "Rien ne s'est passé.",
  "MG-002": "Jamais contre une femme.",
  "MG-003": "Laisse-moi gérer la météo.",
  "MG-004": "Capitaine aux 8000 hommes !",
  "MG-005": "Un monstre pour toi !",
  "MG-006": "Je veux vivre !",
  "MG-007": "SUPER !",
  "MG-008": "Yohohoho !",
};

const ROLE_BY_ID: Record<string, string> = { "MG-006": "Archéologue" };
const TAG_ROLE: Record<string, string> = {
  bretteur: "Bretteur", cuisinier: "Cuisinier", navigateur: "Navigateur", tireur: "Tireur",
  medecin: "Médecin", charpentier: "Charpentier", musicien: "Musicien", archeologue: "Archéologue",
  amiral: "Amiral", capitaine: "Capitaine", sabreur: "Sabreur",
};

/** Job/role shown in the card footer (e.g. "Cuisinier"). */
export function roleOf(def: { id: string; tags?: string[]; faction: string }): string {
  if (ROLE_BY_ID[def.id]) return ROLE_BY_ID[def.id];
  for (const t of def.tags ?? []) if (TAG_ROLE[t]) return TAG_ROLE[t];
  return faction(def.faction).label;
}

/** Per-card identifier code shown in the footer (e.g. "MG-005"). */
export function setCode(id: string): string {
  return id;
}

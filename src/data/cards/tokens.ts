import type { CardDef } from "@/types";

// Token bodies deployed by effects (not part of any deck).
export const tokenCards: CardDef[] = [
  {
    id: "TOK-MARINE", name: "Jeton Marine", type: "character", cost: 0,
    faction: "marine", rarity: "C", set: "TOKEN", isToken: true,
    atk: 2, def: 1, pv: 3, tags: ["marine", "soldat"], preferredRow: "front",
    baseAction: { name: "Tir", atk: 2, description: "Un simple soldat." },
  },
  {
    id: "TOK-AGENT", name: "Agent Baroque Works", type: "character", cost: 0,
    faction: "pirate", rarity: "C", set: "TOKEN", isToken: true,
    atk: 3, def: 1, pv: 3, tags: ["baroque"], preferredRow: "front",
    baseAction: { name: "Frappe", atk: 3, description: "Un agent anonyme." },
  },
  {
    id: "TOK-BANANAWANI", name: "Bananawani", type: "character", cost: 0,
    faction: "pirate", rarity: "C", set: "TOKEN", isToken: true,
    atk: 4, def: 1, pv: 4, tags: ["baroque"], preferredRow: "front",
    baseAction: { name: "Morsure", atk: 4, description: "Un crocodile-banane affamé." },
  },
];

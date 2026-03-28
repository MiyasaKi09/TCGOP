import type { CaptainDef } from "@/types";

export const captainLuffy: CaptainDef = {
  id: "CAP-LUFFY",
  name: "Monkey D. Luffy",
  faction: "pirate",
  tags: ["mugiwara"],
  traits: ["cursed", "conqueror"],

  recto: {
    pv: 30, atk: 4, def: 2,
    passive: {
      name: "Chapeau de paille",
      description: "Chaque Mugiwara en jeu gagne +1 DEF. Si un allie Mugiwara est KO : +2 Vol. bonus.",
      effects: [
        { type: "buffAlly", stat: "def", amount: 1, filter: { tag: "mugiwara" } },
        { type: "onAllyKO", effect: "bonusWill", amount: 2 },
      ],
    },
    attacks: [
      { name: "Gomu Gomu no Pistol", cost: 3, atkBonus: 0, attackTraits: ["range"], description: "Le bras s'etire a distance." },
      { name: "Gomu Gomu no Bazooka", cost: 6, atkBonus: 3, description: "Les deux paumes frappent." },
    ],
  },

  flipCondition: { cost: 4, autoIfAlliesLte: 2 },

  verso: {
    pv: 25, atk: 6, def: 2,
    passive: {
      name: "Celui qui ne tombe pas",
      description: "Immunite Impact. Chaque Mugiwara donne +1 ATK a Luffy. Sous 10 PV : +3 ATK permanent.",
      effects: [{ type: "immuneImpact" }],
    },
    entryEffect: {
      type: "multi",
      effects: [
        { type: "buffAllies", stat: "atk", amount: 3, duration: "turn" },
        { type: "draw", amount: 2 },
      ],
    },
    baseAction: { name: "Gomu Gomu no Gatling", atk: 6, description: "Rafale de poings." },
    specialAttack: {
      name: "Gear Third",
      cost: 4,
      atkBonus: 7,
      attackTraits: ["zone"],
      oncePerGame: false,
      description: "Le poing geant.",
    },
    traits: ["cursed", "conqueror"],
  },
};

export const captainAkainu: CaptainDef = {
  id: "CAP-AKAINU",
  name: "Akainu (Sakazuki)",
  faction: "marine",
  tags: ["marine", "amiral"],
  traits: ["cursed", "conqueror"],

  recto: {
    pv: 35, atk: 5, def: 4,
    passive: {
      name: "Justice Absolue",
      description: "Allies Marines +1 ATK vs personnages avec Prime. Si un Marine allie est KO : l'ennemi subit 2 deg.",
      effects: [
        { type: "buffAlly", stat: "atk", amount: 1, filter: { faction: "marine" } },
      ],
    },
    attacks: [
      { name: "Dai Funka", cost: 3, atkBonus: 0, element: "fire", attackTraits: ["range"], description: "Le poing de magma." },
      { name: "Ryusei Kazan", cost: 7, atkBonus: 5, element: "fire", attackTraits: ["zone"], description: "Les meteores de magma." },
    ],
  },

  flipCondition: { cost: 4, autoIfAlliesLte: 2 },

  verso: {
    pv: 28, atk: 8, def: 3,
    passive: {
      name: "Magma Supreme",
      description: "Intangibilite Logia. Attaques gagnent Feu. Immunite Feu/Brulure. +2 ATK vs Maudits.",
      effects: [{ type: "logiaIntangibility" }],
    },
    entryEffect: {
      type: "multi",
      effects: [
        { type: "damageEnemies", amount: 4, target: "allFront" },
      ],
    },
    baseAction: { name: "Meigo", atk: 8, element: "fire", attackTraits: ["piercing"], description: "Le poing transperce." },
    specialAttack: {
      name: "Inugami Guren",
      cost: 5,
      atkBonus: 7,
      element: "fire",
      oncePerGame: true,
      description: "Le chien de lave devore tout.",
    },
    traits: ["cursed", "logia", "conqueror"],
    naturalHaki: ["armament"],
  },
};

export const allCaptains: CaptainDef[] = [captainLuffy, captainAkainu];

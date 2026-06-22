import type { CaptainDef } from "@/types";

export const captainLuffy: CaptainDef = {
  id: "CAP-LUFFY",
  name: "Monkey D. Luffy",
  faction: "pirate",
  tags: ["mugiwara"],
  traits: ["conqueror"],

  recto: {
    pv: 30, atk: 4, def: 2,
    passive: {
      name: "Pavillon au Chapeau de Paille",
      description: "Vos personnages Pirate gagnent +1 ATK.",
      effects: [
        { type: "buffAlly", stat: "atk", amount: 1, filter: { faction: "pirate" } },
      ],
    },
    // Le Capitaine recto ne peut pas attaquer (Rulebook v3.1 §2.1).
    attacks: [],
  },

  flipCondition: { cost: 2, freeIfAllyKO: true },

  verso: {
    pv: 25, atk: 6, def: 2,
    passive: {
      name: "Esprit de Capitaine",
      description: "Immunisé contre l'Impact. Quand un Mugiwara allié est KO : Luffy gagne +1 ATK permanent (max +3).",
      effects: [
        { type: "immuneImpact" },
        { type: "selfBuffOnAllyKO", stat: "atk", amount: 1, max: 3, filter: { tag: "mugiwara" } },
      ],
    },
    entryEffect: {
      type: "multi",
      effects: [
        { type: "grantSelfRush" },
        { type: "damageEnemies", amount: 3, target: "single" },
      ],
    },
    baseAction: { name: "Gomu Gomu no Pistol", atk: 6, description: "Le poing élastique." },
    specialAttack: {
      name: "Gomu Gomu no Bazooka",
      cost: 3,
      atkBonus: 4,
      attackTraits: ["impact"],
      pushback: true,
      description: "Impact — repousse la cible d'un slot.",
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

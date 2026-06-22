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
  traits: [],

  recto: {
    pv: 35, atk: 5, def: 4,
    passive: {
      name: "Ordre de Marche",
      description: "Vos personnages Marine gagnent +1 ATK.",
      effects: [{ type: "buffAlly", stat: "atk", amount: 1, filter: { faction: "marine" } }],
    },
    attacks: [],
  },

  flipCondition: { cost: 3, freeIfEnemyCursed: true },

  verso: {
    pv: 28, atk: 8, def: 3,
    passive: {
      name: "Justice Implacable",
      description: "Intangibilité Logia. Un ennemi KO par Akainu est banni (retiré du jeu).",
      effects: [{ type: "logiaIntangibility" }, { type: "banishOnKO" }],
    },
    entryEffect: {
      type: "damageEnemies", amount: 4, target: "single", cursedBonus: 6,
    },
    baseAction: { name: "Coup de Magma", atk: 8, element: "fire", description: "Le poing de magma." },
    specialAttack: {
      name: "Ryusei Kazan", cost: 4, atkBonus: 4, element: "fire", attackTraits: ["zone"],
      description: "Les météores de magma. Zone · Feu.",
    },
    traits: ["cursed", "logia"],
  },
};

export const captainCrocodile: CaptainDef = {
  id: "CAP-CROCODILE",
  name: "Crocodile (Mr. 0)",
  faction: "pirate",
  tags: ["baroque"],
  traits: [],

  recto: {
    pv: 30, atk: 5, def: 3,
    passive: {
      name: "Maître de Baroque Works",
      description: "Vos personnages Baroque Works gagnent +1 ATK.",
      effects: [{ type: "buffAlly", stat: "atk", amount: 1, filter: { tag: "baroque" } }],
    },
    attacks: [],
  },

  flipCondition: { cost: 3, freeIfAlliesGte: 3 },

  verso: {
    pv: 25, atk: 7, def: 2,
    passive: {
      name: "Déshydratation",
      description: "Intangibilité Logia. Fin de votre tour : un ennemi blessé perd 1 PV permanent (Sable).",
      effects: [{ type: "logiaIntangibility" }, { type: "endTurnDesiccation", amount: 1 }],
    },
    entryEffect: { type: "damageEnemies", amount: 4, target: "single", sand: true },
    baseAction: { name: "Sables", atk: 7, description: "La main de sable." },
    specialAttack: {
      name: "Desert Girasol", cost: 4, atkBonus: 4, element: "sand", permanentPvLoss: 2,
      description: "La cible perd 2 PV permanent (Sable).",
    },
    traits: ["cursed", "logia"],
  },
};

export const captainShanks: CaptainDef = {
  id: "CAP-SHANKS",
  name: "Shanks (Akagami)",
  faction: "pirate",
  tags: ["redhair"],
  traits: [],

  recto: {
    pv: 35, atk: 6, def: 3,
    passive: {
      name: "Volonté de l'Empereur",
      description: "Vos personnages gagnent +1 ATK.",
      effects: [{ type: "buffAlly", stat: "atk", amount: 1 }],
    },
    attacks: [],
  },

  flipCondition: { cost: 4, freeIfTurnGte: 7 },

  verso: {
    pv: 30, atk: 8, def: 4,
    passive: {
      name: "Présence de l'Empereur",
      description: "Les ennemis adjacents à Shanks ont -2 ATK.",
      effects: [{ type: "debuffAdjacentEnemies", amount: 2 }],
    },
    entryEffect: { type: "haoshoku", immobilizeMaxDef: 2, debuffAtk: 2 },
    baseAction: { name: "Coup de Sabre", atk: 8, description: "Le sabre de l'Empereur (touche les Logia)." },
    specialAttack: {
      name: "Divin Départ", cost: 4, atkBonus: 4, ignoreDef: 99, ignoreShield: true,
      description: "Ignore la DEF et le Bouclier.",
    },
    traits: ["conqueror"],
    naturalHaki: ["armament"],
  },
};

export const allCaptains: CaptainDef[] = [captainLuffy, captainAkainu, captainCrocodile, captainShanks];

import type { CardDef } from "@/types";

export const mugiwaraCards: CardDef[] = [
  // === PERSONNAGES ===
  {
    id: "ST01-001", name: "Roronoa Zoro", type: "character", cost: 5,
    faction: "pirate", rarity: "R", set: "ST01",
    atk: 7, def: 4, pv: 10,
    traits: ["shield"], tags: ["mugiwara", "bretteur"],
    preferredRow: "front",
    passive: {
      name: "Rien ne s'est passe",
      description: "Bouclier. Quand Zoro bloque pour le Capitaine : degats -2.",
      effects: [{ type: "threeWeaponSlots" }],
    },
    synergies: [{ partnerId: "ST01-002", atkBonus: 2, onPartnerKO: 3 }],
    baseAction: { name: "Tatsumaki", atk: 7, description: "La tornade tranchante." },
    specialAttack: { name: "Ashura: Ichibugin", cost: 5, atkBonus: 8, oncePerGame: true, ignoreDef: 2, description: "Neuf sabres." },
  },
  {
    id: "ST01-002", name: "Sanji", type: "character", cost: 4,
    faction: "pirate", rarity: "R", set: "ST01",
    atk: 6, def: 3, pv: 7,
    tags: ["mugiwara", "cuisinier"],
    preferredRow: "front",
    passive: {
      name: "Gentleman Cuisinier",
      description: "NE PEUT PAS attaquer les persos feminins. Debut de tour : chaque allie adjacent +1 PV.",
      effects: [{ type: "cannotAttackFemale" }, { type: "healAdjacent", amount: 1 }],
    },
    synergies: [{ partnerId: "ST01-001", atkBonus: 2, onPartnerKO: 3 }],
    baseAction: { name: "Collier Shoot", atk: 6, description: "Coup de pied circulaire." },
    specialAttack: { name: "Diable Jambe: Concasse", cost: 3, atkBonus: 4, element: "fire", attackTraits: ["zone"], description: "Cible + adjacents." },
  },
  {
    id: "ST01-003", name: "Nami", type: "character", cost: 2,
    faction: "pirate", rarity: "C", set: "ST01",
    atk: 1, def: 1, pv: 4,
    traits: ["stealth", "range"], tags: ["mugiwara", "navigateur"],
    preferredRow: "back",
    passive: {
      name: "Art de la navigation",
      description: "Debut de tour : regarde la carte du dessus de ton deck.",
      effects: [],
    },
    baseAction: { name: "Mirage Tempo", atk: 0, isSupport: true, description: "1 allie inciblable ce tour." },
    specialAttack: { name: "Thunder Tempo", cost: 1, atkBonus: 2, element: "thunder", attackTraits: ["range"], description: "Cible n'importe quel ennemi." },
  },
  {
    id: "ST01-004", name: "Usopp", type: "character", cost: 2,
    faction: "pirate", rarity: "C", set: "ST01",
    atk: 1, def: 1, pv: 4,
    traits: ["stealth", "range"], tags: ["mugiwara", "tireur"],
    preferredRow: "back",
    passive: {
      name: "Tireur d'elite",
      description: "Ignore Furtif ennemi.",
      effects: [{ type: "ignoreEnemyStealth" }],
    },
    baseAction: { name: "Piege Etoile", atk: 0, isSupport: true, description: "Pose 1 piege (3 degats)." },
    specialAttack: { name: "Hissatsu Atlas Suisei", cost: 3, atkBonus: 6, attackTraits: ["range"], description: "Repousse la cible d'1 slot." },
  },
  {
    id: "ST01-005", name: "Tony Tony Chopper", type: "character", cost: 2,
    faction: "pirate", rarity: "C", set: "ST01",
    atk: 1, def: 2, pv: 6,
    traits: ["stealth", "cursed"], tags: ["mugiwara", "medecin"],
    preferredRow: "back",
    passive: {
      name: "Medecin de bord",
      description: "Debut de tour : retire 1 Brulure OU 1 Poison d'un allie adjacent.",
      effects: [],
    },
    baseAction: { name: "Premiers Soins", atk: 0, isSupport: true, healAmount: 2, description: "Soigne 2 PV a 1 allie adjacent." },
    specialAttack: { name: "Monster Point", cost: 5, atkBonus: 9, oncePerGame: true, description: "ATK fixe 10, +4 DEF, 2 tours puis KO." },
  },
  {
    id: "ST01-006", name: "Nico Robin", type: "character", cost: 3,
    faction: "pirate", rarity: "R", set: "ST01",
    atk: 3, def: 2, pv: 5,
    traits: ["stealth", "range", "cursed"], tags: ["mugiwara"],
    preferredRow: "back",
    passive: {
      name: "Cien Fleur",
      description: "Portee illimitee sur toutes ses actions.",
      effects: [],
    },
    baseAction: { name: "Cien Fleur: Deluxe", atk: 0, isSupport: true, immobilize: true, description: "Immobilise 1 ennemi (Portee)." },
    specialAttack: { name: "Seis Fleur: Clutch", cost: 2, atkBonus: 2, attackTraits: ["range"], description: "Bloque la speciale de la cible 1 tour." },
  },
  {
    id: "ST01-007", name: "Franky", type: "character", cost: 3,
    faction: "pirate", rarity: "U", set: "ST01",
    atk: 5, def: 4, pv: 8,
    tags: ["mugiwara", "charpentier"],
    preferredRow: "front",
    passive: {
      name: "Cyborg",
      description: "2 slots Accessoire. Si Navire en jeu : +1 PV Navire/tour. Defausse 1 carte = +2 ATK ce tour.",
      effects: [{ type: "twoAccessorySlots" }],
    },
    baseAction: { name: "Weapons Left", atk: 5, element: "fire", attackTraits: ["range"], description: "Tir de bras canon." },
    specialAttack: { name: "Coup de Vent", cost: 3, atkBonus: 4, attackTraits: ["range", "zone"], description: "Cible + adjacents 3 deg." },
  },
  {
    id: "ST01-008", name: "Brook", type: "character", cost: 3,
    faction: "pirate", rarity: "U", set: "ST01",
    atk: 5, def: 2, pv: 6,
    traits: ["cursed"], tags: ["mugiwara", "musicien"],
    preferredRow: "front",
    passive: {
      name: "Revenant",
      description: "1x/partie : si Brook est KO, il revient avec 3 PV au tour suivant.",
      effects: [{ type: "revive", pv: 3 }],
    },
    baseAction: { name: "New World", atk: 0, isSupport: true, description: "Musique : tous allies +1 ATK et +1 DEF ce tour." },
    specialAttack: { name: "Hanauta Sancho: Yahazu Giri", cost: 2, atkBonus: 2, element: "ice", description: "Degats retardes." },
  },

  // === OBJETS - ARMES ===
  {
    id: "ST01-009", name: "Wado Ichimonji", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 2, bonusDef: 0, restriction: "bretteur",
    equipEffect: "Si equipe par Zoro : +1 DEF.",
  },
  {
    id: "ST01-010", name: "Sandai Kitetsu", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 2, restriction: "bretteur",
    equipEffect: "MALEDICTION : 50% chance 1 degat/tour. Zoro : jamais active.",
  },
  {
    id: "ST01-011", name: "Yubashiri", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 1, restriction: "bretteur",
    equipEffect: "Si detruite : Zoro +1 ATK permanent.",
  },
  {
    id: "ST01-012", name: "Clima-Tact", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 2, restriction: "Nami",
    equipEffect: "Thunder Tempo gagne Zone.",
  },
  {
    id: "ST01-013", name: "Kabuto", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 2, restriction: "Usopp",
    equipEffect: "Les attaques d'Usopp ne peuvent plus etre esquivees.",
  },

  // === OBJETS - FRUITS ===
  {
    id: "ST01-014", name: "Gomu Gomu no Mi", type: "object", subtype: "fruit", cost: 3,
    faction: "pirate", rarity: "SR", set: "ST01",
    bonusAtk: 0, restriction: "Luffy",
    equipEffect: "Maudit, Immunite Impact, Portee. Eveil: Joy Boy.",
  },
  {
    id: "ST01-015", name: "Hana Hana no Mi", type: "object", subtype: "fruit", cost: 2,
    faction: "pirate", rarity: "R", set: "ST01",
    bonusAtk: 0, restriction: "Robin",
    equipEffect: "Maudit, Portee. Immobilise 1 ennemi/tour.",
  },
  {
    id: "ST01-016", name: "Yomi Yomi no Mi", type: "object", subtype: "fruit", cost: 2,
    faction: "pirate", rarity: "R", set: "ST01",
    bonusAtk: 0, restriction: "Brook",
    equipEffect: "Maudit. Resurrection 1x/partie.",
  },

  // === OBJETS - ACCESSOIRES ===
  {
    id: "ST01-017", name: "Baril d'Eau", type: "object", subtype: "accessory", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 0,
    equipEffect: "Usage unique. Attaque gagne trait Eau (x2 vs Maudits, touche Logia).",
  },
  {
    id: "ST01-018", name: "Dial d'Impact", type: "object", subtype: "accessory", cost: 2,
    faction: "pirate", rarity: "U", set: "ST01",
    bonusAtk: 0,
    equipEffect: "Absorbe 1 attaque (max 5 deg.). Tour suivant : relache les degats.",
  },
  {
    id: "ST01-019", name: "Vivre Card", type: "object", subtype: "accessory", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 0,
    equipEffect: "Quand un Mugiwara KO : revele dessus du deck, si Mugiwara deploie gratuit.",
  },

  // === NAVIRES ===
  {
    id: "ST01-020", name: "Going Merry", type: "ship", cost: 1,
    faction: "pirate", rarity: "U", set: "ST01",
    shipPassive: "Mugiwara gagnent +1 PV au deploiement.",
    shipActive: { name: "Dernier voyage", cost: 0, description: "Si detruit : tous allies +2 ATK et +2 DEF ce tour.", oncePerGame: true },
  },
  {
    id: "ST01-021", name: "Thousand Sunny", type: "ship", cost: 2,
    faction: "pirate", rarity: "R", set: "ST01",
    shipPassive: "Mugiwara gagnent +1 PV et +1 ATK au deploiement.",
    shipActive: { name: "Gaon Cannon", cost: 3, description: "1x/partie : 4 deg. a toute la Ligne Avant ennemie.", oncePerGame: true },
  },

  // === EVENEMENTS ===
  {
    id: "ST01-022", name: "Volonte du D", type: "event", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    eventEffect: { type: "gainWill", amount: 3 },
  },
  {
    id: "ST01-023", name: "Nakama !", type: "event", cost: 2,
    faction: "pirate", rarity: "U", set: "ST01",
    eventEffect: { type: "healAlly", amount: 3, allAllies: false },
  },
  {
    id: "ST01-024", name: "Flashback Promesse", type: "event", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    eventEffect: { type: "draw", amount: 2, discard: 1 },
  },
  {
    id: "ST01-025", name: "Tempete", type: "event", cost: 3,
    faction: "pirate", rarity: "U", set: "ST01",
    eventEffect: { type: "damageEnemies", amount: 2, target: "allCursed" },
  },
  {
    id: "ST01-028", name: "Coup de Burst", type: "event", cost: 3,
    faction: "pirate", rarity: "R", set: "ST01",
    eventEffect: { type: "dodgeAll" },
  },

  // === COUNTERS ===
  {
    id: "ST01-026", name: "JE VEUX VIVRE !", type: "counter", cost: 0,
    faction: "pirate", rarity: "R", set: "ST01",
    counterEffect: { type: "survive", description: "Un allie Mugiwara survit avec 1 PV." },
  },
  {
    id: "ST01-027", name: "Drapeau Noir", type: "counter", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    counterEffect: { type: "reduceDamage", amount: 4, captainBonus: 6 },
  },
];

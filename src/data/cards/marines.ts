import type { CardDef } from "@/types";

// ST02 — MARINES : JUSTICE ABSOLUE — conforme Rulebook v3.1 / ST02_Marines_v3.1
export const marinesCards: CardDef[] = [
  // === PERSONNAGES ===
  {
    id: "MR-001", name: "Coby", type: "character", cost: 2,
    faction: "marine", rarity: "C", set: "ST02",
    atk: 2, def: 1, pv: 4, tags: ["marine", "soldat"], preferredRow: "front",
    synergies: [{ partnerId: "MR-002", atkBonus: 1 }],
    baseAction: { name: "Coup de Poing", atk: 2, description: "Le futur amiral s'entraîne." },
    specialAttack: { name: "Poing de la Justice", cost: 2, atkBonus: 3, attackTraits: ["piercing"], description: "Perçant." },
  },
  {
    id: "MR-002", name: "Helmeppo", type: "character", cost: 2,
    faction: "marine", rarity: "C", set: "ST02",
    atk: 3, def: 2, pv: 4, tags: ["marine", "soldat", "bretteur"], preferredRow: "front",
    synergies: [{ partnerId: "MR-001", atkBonus: 1 }],
    baseAction: { name: "Coup de Lame", atk: 3, description: "Le fils prodigue." },
    specialAttack: { name: "Frappe Jumelle", cost: 2, atkBonus: 2, twoTargets: true, description: "Touche 2 cibles." },
  },
  {
    id: "MR-003", name: "Tashigi", type: "character", cost: 3,
    faction: "marine", rarity: "U", set: "ST02",
    atk: 4, def: 2, pv: 5, tags: ["marine", "capitaine_marine", "bretteur"], preferredRow: "front",
    passive: { name: "Chasseuse de Meito", description: "Peut équiper 2 Armes au lieu d'une.", effects: [{ type: "twoWeaponSlots" }] },
    baseAction: { name: "Coup de Sabre", atk: 4, description: "La lame précise." },
    specialAttack: { name: "Tenpô", cost: 2, atkBonus: 3, attackTraits: ["piercing"], description: "Perçant." },
  },
  {
    id: "MR-004", name: "Smoker", type: "character", cost: 4,
    faction: "marine", rarity: "R", set: "ST02",
    atk: 5, def: 3, pv: 7, tags: ["marine", "capitaine_marine"], preferredRow: "front",
    passive: { name: "White Snake", description: "Les attaques de Smoker ignorent le Furtif et l'Inciblable.", effects: [{ type: "stripStealthOnAttack" }] },
    baseAction: { name: "Jitte", atk: 5, description: "La matraque en granit marin." },
    specialAttack: { name: "White Blow", cost: 2, atkBonus: 3, immobilize: true, description: "La cible perd sa prochaine action." },
  },
  {
    id: "MR-005", name: "Sentomaru", type: "character", cost: 3,
    faction: "marine", rarity: "U", set: "ST02",
    atk: 4, def: 4, pv: 7, tags: ["marine", "garde"], preferredRow: "front",
    naturalHaki: ["armament"],
    passive: { name: "Garde du Corps", description: "Haki Armement naturel (touche les Logia).", effects: [{ type: "naturalHaki", hakiType: "armament" }] },
    baseAction: { name: "Coup de Hache", atk: 4, description: "La hache de la garde." },
    specialAttack: { name: "Ashigara Dokkoi", cost: 3, atkBonus: 3, ignoreShield: true, description: "Ignore le Bouclier." },
  },
  {
    id: "MR-006", name: "Momonga", type: "character", cost: 3,
    faction: "marine", rarity: "U", set: "ST02",
    atk: 5, def: 3, pv: 6, tags: ["marine", "vice_amiral", "bretteur"], preferredRow: "front",
    passive: { name: "Volonté de Fer", description: "Immunisé contre tous les effets de contrôle.", effects: [{ type: "immuneControl" }] },
    baseAction: { name: "Coup de Sabre", atk: 5, description: "La volonté triomphe de la chair." },
    specialAttack: { name: "Frappe Disciplinée", cost: 2, atkBonus: 3, attackTraits: ["piercing"], description: "Perçant." },
  },
  {
    id: "MR-007", name: "Garp", type: "character", cost: 5,
    faction: "marine", rarity: "SR", set: "ST02",
    atk: 7, def: 3, pv: 10, tags: ["marine", "vice_amiral"], preferredRow: "front",
    naturalHaki: ["armament"],
    passive: { name: "Poing de Garp", description: "Haki Armement naturel (toutes ses attaques touchent les Logia).", effects: [{ type: "naturalHaki", hakiType: "armament" }] },
    baseAction: { name: "Poing de l'Amour", atk: 7, attackTraits: ["impact"], description: "Le poing d'amour de grand-père." },
    specialAttack: { name: "Boulet de Canon", cost: 3, atkBonus: 3, attackTraits: ["range", "impact"], pushback: true, description: "Portée · Impact." },
  },
  {
    id: "MR-008", name: "Kizaru (Borsalino)", type: "character", cost: 5,
    faction: "marine", rarity: "SR", set: "ST02",
    atk: 6, def: 2, pv: 8, traits: ["cursed", "logia"], tags: ["marine", "amiral"], preferredRow: "front",
    passive: { name: "Vitesse de la Lumière", description: "Les attaques de Kizaru ne peuvent pas être esquivées.", effects: [{ type: "logiaIntangibility" }, { type: "noDodge" }] },
    baseAction: { name: "Coup de Pied Lumineux", atk: 6, description: "La vitesse de la lumière." },
    specialAttack: { name: "Yasakani no Magatama", cost: 4, atkBonus: 4, attackTraits: ["zone", "range"], description: "La pluie de lasers." },
  },
  {
    id: "MR-009", name: "Aokiji (Kuzan)", type: "character", cost: 5,
    faction: "marine", rarity: "SR", set: "ST02",
    atk: 6, def: 3, pv: 9, traits: ["cursed", "logia"], tags: ["marine", "amiral"], preferredRow: "front",
    passive: { name: "Souffle Glacial", description: "Intangibilité Logia. Toutes ses attaques ont l'élément Glace.", effects: [{ type: "logiaIntangibility" }] },
    baseAction: { name: "Ice Time", atk: 6, element: "ice", description: "La cible perd sa prochaine action." },
    specialAttack: { name: "Ice Age", cost: 4, atkBonus: 3, element: "ice", attackTraits: ["zone"], description: "Le gel se propage." },
  },
  {
    id: "MR-010", name: "Sengoku", type: "character", cost: 5,
    faction: "marine", rarity: "SR", set: "ST02",
    atk: 6, def: 4, pv: 9, tags: ["marine", "amiral_en_chef"], preferredRow: "front",
    passive: { name: "Stratège Suprême", description: "Vos personnages Marine gagnent +1 DEF.", effects: [{ type: "buffAlly", stat: "def", amount: 1, filter: { faction: "marine" } }] },
    baseAction: { name: "Onde de Choc", atk: 6, description: "Le Bouddha frappe." },
    specialAttack: { name: "Daibutsu : Paume de l'Illumination", cost: 4, atkBonus: 4, attackTraits: ["zone", "impact"], description: "Zone · Impact." },
  },

  // === OBJETS - FRUITS DU DÉMON ===
  {
    id: "MR-011", name: "Magu Magu no Mi", type: "object", subtype: "fruit", cost: 3,
    faction: "marine", rarity: "SR", set: "ST02", bonusAtk: 0, restriction: "Akainu",
    equipEffect: "Applique Logia + Maudit. Attaques gagnent Feu.",
    fruitEffects: {
      base: { grantsTraits: ["cursed", "logia"], passiveDescription: "Logia. Meigo : 2 Vol · ATK +3 · Feu." },
      awakening: {
        porteurLegitime: "Akainu", minTurns: 5, volCost: 3, atkBonus: 4,
        passiveDescription: "Terre Brûlée — les ennemis adjacents subissent 1 dégât/tour (Feu).",
        specialAttack: { name: "Inugami Guren", cost: 4, atkBonus: 6, oncePerGame: true, element: "fire", attackTraits: ["zone", "piercing"], description: "Zone · Perçant · Feu." },
      },
    },
  },
  {
    id: "MR-012", name: "Moku Moku no Mi", type: "object", subtype: "fruit", cost: 2,
    faction: "marine", rarity: "R", set: "ST02", bonusAtk: 0, restriction: "Smoker",
    equipEffect: "Applique Logia + Maudit.",
    fruitEffects: {
      base: { grantsTraits: ["cursed", "logia"], passiveDescription: "Logia. White Out : 1 Vol · ATK +2 · la cible ne peut pas se déplacer." },
      awakening: {
        porteurLegitime: "Smoker", minTurns: 5, volCost: 2, atkBonus: 3,
        passiveDescription: "White Monster — les ennemis ne peuvent pas quitter un slot adjacent au porteur.",
        specialAttack: { name: "White Launcher", cost: 3, atkBonus: 3, attackTraits: ["range"], immobilize: true, description: "Portée · immobilise 1 tour." },
      },
    },
  },

  // === OBJETS - ARMES ===
  {
    id: "MR-013", name: "Shigure", type: "object", subtype: "weapon", cost: 1,
    faction: "marine", rarity: "C", set: "ST02", bonusAtk: 1, restriction: "bretteur",
    equipEffect: "+1 ATK. Si équipée par Tashigi : +1 ATK de plus.",
  },
  {
    id: "MR-014", name: "Jitte Granit Marin", type: "object", subtype: "weapon", cost: 1,
    faction: "marine", rarity: "U", set: "ST02", bonusAtk: 1, grantsElement: "water",
    equipEffect: "+1 ATK. Granit Marin : x2 dégâts aux Maudits et touche les Logia.",
  },

  // === OBJETS - ACCESSOIRES ===
  {
    id: "MR-015", name: "Menottes Granit Marin", type: "object", subtype: "accessory", cost: 2,
    faction: "marine", rarity: "R", set: "ST02", bonusAtk: 0,
    equipEffect: "Actif (1x/partie) : un ennemi Maudit perd ses traits et son action à son prochain tour.",
  },
  {
    id: "MR-016", name: "Canon Marine", type: "object", subtype: "accessory", cost: 1,
    faction: "marine", rarity: "C", set: "ST02", bonusAtk: 0,
    equipEffect: "Le porteur gagne une attaque : Tir de Canon — 1 Vol · 3 dégâts (Portée).",
  },
  {
    id: "MR-017", name: "Boulet Granit Marin", type: "object", subtype: "accessory", cost: 1,
    faction: "marine", rarity: "C", set: "ST02", bonusAtk: 0,
    equipEffect: "1x/partie : 2 dégâts à un ennemi ; s'il est Maudit, 4 dégâts et il perd ses traits ce tour.",
  },

  // === NAVIRES ===
  {
    id: "MR-018", name: "Navire de Guerre", type: "ship", cost: 2,
    faction: "marine", rarity: "U", set: "ST02",
    shipPassive: "Vos Marine gagnent +1 ATK.",
    shipActive: { name: "Salve de Canons", cost: 2, description: "3 dégâts à un ennemi (Portée).", oncePerGame: true },
  },
  {
    id: "MR-019", name: "Navire de Justice", type: "ship", cost: 1,
    faction: "marine", rarity: "C", set: "ST02",
    shipPassive: "Vos Marine gagnent +1 PV.",
    shipDestroyEffect: { deployToken: "TOK-MARINE" },
  },

  // === EVENEMENTS ===
  {
    id: "MR-020", name: "Buster Call", type: "event", cost: 5,
    faction: "marine", rarity: "SR", set: "ST02",
    eventEffect: { type: "damageEnemies", amount: 4, target: "all", destroyShips: true },
  },
  {
    id: "MR-021", name: "Promotion", type: "event", cost: 1,
    faction: "marine", rarity: "C", set: "ST02",
    eventEffect: { type: "buffSingle", stat: "atk", amount: 1, duration: "permanent" },
  },
  {
    id: "MR-022", name: "Justice Absolue", type: "event", cost: 2,
    faction: "marine", rarity: "U", set: "ST02",
    eventEffect: { type: "buffAllies", stat: "atk", amount: 2, filter: { faction: "marine" }, duration: "turn" },
  },
  {
    id: "MR-023", name: "Renforts", type: "event", cost: 1,
    faction: "marine", rarity: "C", set: "ST02",
    eventEffect: { type: "deployTokens", tokenId: "TOK-MARINE", count: 2 },
  },
  {
    id: "MR-024", name: "Ordre de Tir", type: "event", cost: 2,
    faction: "marine", rarity: "U", set: "ST02",
    eventEffect: { type: "custom", id: "coordinatedFire", description: "Chaque Marine inflige 1 dégât à un ennemi." },
  },
  {
    id: "MR-027", name: "Embargo", type: "event", cost: 2,
    faction: "marine", rarity: "U", set: "ST02",
    eventEffect: { type: "custom", id: "embargo", description: "L'adversaire ne peut ni équiper ni jouer de Navire à son prochain tour." },
  },
  {
    id: "MR-028", name: "Exécution Publique", type: "event", cost: 3,
    faction: "marine", rarity: "R", set: "ST02",
    eventEffect: { type: "custom", id: "execute4", description: "Détruisez un ennemi de PV actuels ≤ 4." },
  },

  // === COUNTERS ===
  {
    id: "MR-025", name: "Manteau de Justice", type: "counter", cost: 0,
    faction: "marine", rarity: "R", set: "ST02",
    counterEffect: { type: "cancel", description: "Annule l'attaque. 1x par partie.", once: true },
  },
  {
    id: "MR-026", name: "Mur d'Acier", type: "counter", cost: 1,
    faction: "marine", rarity: "C", set: "ST02",
    counterEffect: { type: "reduceDamage", amount: 5 },
  },
];

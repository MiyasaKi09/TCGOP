import type { CardDef } from "@/types";

// ST01 — MUGIWARA PRE-ELLIPSE — conforme Rulebook v3.1 / ST01_Mugiwara_v3.1
export const mugiwaraCards: CardDef[] = [
  // === PERSONNAGES ===
  {
    id: "MG-001", name: "Roronoa Zoro", type: "character", cost: 5,
    faction: "pirate", rarity: "R", set: "ST01",
    atk: 7, def: 3, pv: 10,
    tags: ["mugiwara", "bretteur"],
    preferredRow: "front",
    // Pas de passif — la Rivalité est une synergie.
    synergies: [{ partnerId: "MG-002", atkBonus: 2 }],
    baseAction: { name: "Coup de Sabre", atk: 7, description: "Trois sabres, une seule volonté." },
    specialAttack: { name: "Oni Giri", cost: 3, atkBonus: 3, attackTraits: ["piercing"], description: "La coupe du démon. Perçant." },
  },
  {
    id: "MG-002", name: "Sanji", type: "character", cost: 4,
    faction: "pirate", rarity: "R", set: "ST01",
    atk: 6, def: 3, pv: 7,
    tags: ["mugiwara", "cuisinier"],
    preferredRow: "front",
    passive: {
      name: "Galanterie",
      description: "NE PEUT PAS cibler les personnages féminins.",
      effects: [{ type: "cannotAttackFemale" }],
    },
    synergies: [{ partnerId: "MG-001", atkBonus: 2 }],
    baseAction: { name: "Coup de Pied", atk: 6, description: "La Jambe Noire frappe." },
    specialAttack: { name: "Diable Jambe", cost: 3, atkBonus: 3, element: "fire", description: "La jambe en feu." },
  },
  {
    id: "MG-003", name: "Nami", type: "character", cost: 2,
    faction: "pirate", rarity: "C", set: "ST01",
    atk: 1, def: 1, pv: 4,
    traits: ["range"], tags: ["mugiwara", "navigateur", "female"],
    preferredRow: "back",
    baseAction: { name: "Prévisions", atk: 0, isSupport: true, scry: 2, description: "Regarde les 2 cartes du dessus de ton deck, remets-les dans l'ordre voulu." },
    specialAttack: { name: "Thunder Tempo", cost: 2, atkBonus: 3, element: "thunder", description: "La foudre frappe." },
  },
  {
    id: "MG-004", name: "Usopp", type: "character", cost: 2,
    faction: "pirate", rarity: "C", set: "ST01",
    atk: 1, def: 1, pv: 4,
    traits: ["range"], tags: ["mugiwara", "tireur"],
    preferredRow: "back",
    passive: {
      name: "Vantardise",
      description: "Début de votre tour : un de vos alliés gagne +1 ATK ce tour.",
      effects: [{ type: "startTurnBuffAlly", stat: "atk", amount: 1 }],
    },
    baseAction: { name: "Bluff", atk: 0, isSupport: true, bluff: true, description: "Un ennemi de DEF ≤ 1 perd sa prochaine action (il a peur)." },
    specialAttack: { name: "Kayaku Boshi", cost: 2, atkBonus: 3, element: "fire", description: "L'étoile explosive." },
  },
  {
    id: "MG-005", name: "Tony Tony Chopper", type: "character", cost: 2,
    faction: "pirate", rarity: "C", set: "ST01",
    atk: 1, def: 2, pv: 6,
    tags: ["mugiwara", "medecin"],
    preferredRow: "back",
    passive: {
      name: "Médecin de bord",
      description: "Début de votre tour : soigne 1 PV à un allié adjacent.",
      effects: [{ type: "healAdjacent", amount: 1 }],
    },
    baseAction: { name: "Point de Suture", atk: 0, isSupport: true, healAmount: 2, description: "Soigne 2 PV à un allié (n'importe quel slot)." },
    specialAttack: { name: "Monster Point", cost: 3, atkBonus: 0, oncePerGame: true, isSupport: true, transform: { atk: 8, def: 2, pv: 6, turns: 2 }, description: "ATK 8 · DEF 2 · PV 6 + Rush pendant 2 tours, puis KO automatique." },
  },
  {
    id: "MG-006", name: "Nico Robin", type: "character", cost: 3,
    faction: "pirate", rarity: "R", set: "ST01",
    atk: 3, def: 2, pv: 5,
    traits: ["range"], tags: ["mugiwara", "female", "archeologue"],
    preferredRow: "back",
    passive: {
      name: "Hana Hana",
      description: "Les attaques de Robin ne peuvent pas être esquivées.",
      effects: [{ type: "noDodge" }],
    },
    baseAction: { name: "Seis Fleur", atk: 3, description: "Des bras surgissent de partout." },
    specialAttack: { name: "Clutch", cost: 2, atkBonus: 2, immobilize: true, description: "La cible est immobilisée 1 tour." },
  },
  {
    id: "MG-007", name: "Franky", type: "character", cost: 3,
    faction: "pirate", rarity: "U", set: "ST01",
    atk: 5, def: 4, pv: 8,
    traits: ["shield"], tags: ["mugiwara", "charpentier"],
    preferredRow: "front",
    baseAction: { name: "Strong Right", atk: 5, description: "Le poing cyborg." },
    specialAttack: { name: "Coup de Vent", cost: 3, atkBonus: 4, element: "fire", attackTraits: ["range"], description: "Tir d'air comprimé à distance." },
  },
  {
    id: "MG-008", name: "Brook", type: "character", cost: 3,
    faction: "pirate", rarity: "U", set: "ST01",
    atk: 5, def: 2, pv: 6,
    tags: ["mugiwara", "musicien", "bretteur"],
    preferredRow: "front",
    passive: {
      name: "Le Barde",
      description: "Tant que Brook est en jeu, vos alliés gagnent +1 DEF.",
      effects: [{ type: "buffAlly", stat: "def", amount: 1 }],
    },
    baseAction: { name: "Mélodie de l'Âme", atk: 0, isSupport: true, buffAllyAtk: 2, description: "Un allié gagne +2 ATK ce tour." },
    specialAttack: { name: "Hanauta Sancho: Yahazu Giri", cost: 3, atkBonus: 3, element: "ice", description: "Un coup glacial." },
  },

  // === OBJETS - ARMES ===
  {
    id: "MG-009", name: "Wado Ichimonji", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 1, restriction: "bretteur",
    equipEffect: "+1 ATK. Si équipée par Zoro : +1 DEF de plus.",
  },
  {
    id: "MG-010", name: "Sandai Kitetsu", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 2, restriction: "bretteur",
    equipEffect: "+2 ATK. Malédiction : au début de votre tour, le porteur subit 1 dégât.",
  },
  {
    id: "MG-011", name: "Yubashiri", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 1, restriction: "bretteur",
    equipEffect: "+1 ATK. Si détruite : le porteur gagne +1 ATK permanent.",
  },
  {
    id: "MG-012", name: "Clima-Tact", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 1, restriction: "Nami", grantsElement: "thunder",
    equipEffect: "+1 ATK. Attaques du porteur : élément Foudre. Combo (Usopp + Nami en jeu) : coûte 0.",
  },
  {
    id: "MG-013", name: "Kabuto", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 1, restriction: "tireur",
    equipEffect: "+1 ATK. Les attaques du porteur sont inesquivables (ignorent l'esquive et l'Observation).",
  },

  // === OBJETS - FRUITS DU DÉMON ===
  {
    id: "MG-014", name: "Gomu Gomu no Mi", type: "object", subtype: "fruit", cost: 3,
    faction: "pirate", rarity: "SR", set: "ST01",
    bonusAtk: 0, restriction: "Luffy",
    equipEffect: "Maudit. Immunité Impact. Portée sur les attaques.",
    fruitEffects: {
      base: {
        grantsTraits: ["cursed"],
        passiveDescription: "Immunité Impact. Gomu Gomu no Pistol : 1 Vol · ATK +3 · Impact.",
      },
      awakening: {
        porteurLegitime: "Luffy",
        minTurns: 5,
        volCost: 3,
        atkBonus: 3,
        grantsTraits: ["rush"],
        passiveDescription: "Gear 4 Boundman — +3 ATK, Rush, Impact sur toutes les attaques.",
        specialAttack: { name: "Kong Gun", cost: 4, atkBonus: 6, oncePerGame: true, description: "Impact · Zone · ignore le Bouclier." },
      },
    },
  },
  {
    id: "MG-015", name: "Hana Hana no Mi", type: "object", subtype: "fruit", cost: 2,
    faction: "pirate", rarity: "R", set: "ST01",
    bonusAtk: 0, restriction: "Robin",
    equipEffect: "Maudit. Le porteur gagne Portée.",
    fruitEffects: {
      base: {
        grantsTraits: ["cursed"],
        passiveDescription: "Le porteur gagne Portée. Doce Fleur : 1 Vol · ATK +2 · repositionne la cible.",
      },
      awakening: {
        porteurLegitime: "Robin",
        minTurns: 5,
        volCost: 2,
        atkBonus: 0,
        passiveDescription: "Mil Fleur — Portée ; un ennemi Avant ne peut pas utiliser Bouclier à votre tour.",
        specialAttack: { name: "Gigante Fleur", cost: 3, atkBonus: 0, description: "Immobilise toute la Ligne Avant adverse 1 tour + 2 dégâts à chacun." },
      },
    },
  },
  {
    id: "MG-016", name: "Yomi Yomi no Mi", type: "object", subtype: "fruit", cost: 2,
    faction: "pirate", rarity: "R", set: "ST01",
    bonusAtk: 0, restriction: "Brook",
    equipEffect: "Maudit. Résurrection 1x/partie (revient 3 PV).",
    fruitEffects: {
      base: {
        grantsTraits: ["cursed"],
        passiveDescription: "Revenant : si KO, revient (même slot) avec 3 PV — 1x/partie. Aubade Coup Droit : 1 Vol · ATK +2 · Glace.",
      },
      awakening: {
        porteurLegitime: "Brook",
        minTurns: 5,
        volCost: 2,
        atkBonus: 0,
        passiveDescription: "Soul King — Revenant + les ennemis adjacents au porteur −1 ATK.",
        specialAttack: { name: "Nemuriuta Flanc", cost: 3, atkBonus: 0, description: "Un ennemi est endormi (perd ses 2 prochaines actions)." },
      },
    },
  },

  // === OBJETS - ACCESSOIRES ===
  {
    id: "MG-017", name: "Baril d'Eau", type: "object", subtype: "accessory", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 0, grantsElement: "water",
    equipEffect: "Attaques du porteur : élément Eau (x2 vs Maudits ; touche les Logia).",
  },
  {
    id: "MG-018", name: "Dial d'Impact", type: "object", subtype: "accessory", cost: 2,
    faction: "pirate", rarity: "U", set: "ST01",
    bonusAtk: 0,
    equipEffect: "1x/partie : quand le porteur subit une attaque, absorbe les dégâts (→ 0). À votre prochain tour, infligez ce montant à un ennemi (Impact).",
  },
  {
    id: "MG-019", name: "Vivre Card", type: "object", subtype: "accessory", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    bonusAtk: 0,
    equipEffect: "Si le porteur est KO : cherchez un personnage Mugiwara de coût ≤ 3 dans votre deck et ajoutez-le à votre main.",
  },

  // === NAVIRES ===
  {
    id: "MG-020", name: "Going Merry", type: "ship", cost: 1,
    faction: "pirate", rarity: "U", set: "ST01",
    shipPassive: "Vos Mugiwara ont +1 PV au déploiement.",
    shipDestroyEffect: { healAll: 2, draw: 1 },
  },
  {
    id: "MG-021", name: "Thousand Sunny", type: "ship", cost: 2,
    faction: "pirate", rarity: "R", set: "ST01",
    shipPassive: "Vos Mugiwara ont +1 ATK.",
    shipActive: { name: "Gaon Cannon", cost: 3, description: "5 dégâts à un ennemi (Portée).", oncePerGame: true },
  },

  // === EVENEMENTS ===
  {
    id: "MG-022", name: "Volonté du D.", type: "event", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    eventEffect: { type: "gainWill", amount: 2 },
  },
  {
    id: "MG-023", name: "Nakama !", type: "event", cost: 2,
    faction: "pirate", rarity: "U", set: "ST01",
    eventEffect: { type: "rally", atk: 1, def: 1, heal: 1 },
  },
  {
    id: "MG-024", name: "Flashback : Promesse", type: "event", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    eventEffect: { type: "buffSingle", stat: "atk", amount: 3, duration: "permanent", requiresOwnKO: true },
  },
  {
    id: "MG-025", name: "Tempête", type: "event", cost: 3,
    faction: "pirate", rarity: "U", set: "ST01",
    eventEffect: { type: "damageEnemies", amount: 2, target: "all", cursedBonus: 3 },
  },
  {
    id: "MG-028", name: "Coup de Burst", type: "event", cost: 3,
    faction: "pirate", rarity: "R", set: "ST01",
    eventEffect: { type: "rushBuff", atk: 3 },
  },

  // === COUNTERS ===
  {
    id: "MG-026", name: "JE VEUX VIVRE !", type: "counter", cost: 0,
    faction: "pirate", rarity: "R", set: "ST01",
    counterEffect: { type: "survive", description: "Un allié Mugiwara survit avec 1 PV. Une fois par personnage." },
  },
  {
    id: "MG-027", name: "Drapeau Noir", type: "counter", cost: 1,
    faction: "pirate", rarity: "C", set: "ST01",
    counterEffect: { type: "reduceDamage", amount: 4 },
  },
];

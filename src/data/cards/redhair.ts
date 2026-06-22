import type { CardDef } from "@/types";

// ST04 — RED HAIR : L'EMPEREUR — conforme Rulebook v3.1 / ST04_RedHair_v3.1
export const redhairCards: CardDef[] = [
  // === PERSONNAGES ===
  {
    id: "RH-001", name: "Ben Beckman", type: "character", cost: 5,
    faction: "pirate", rarity: "SR", set: "ST04",
    atk: 6, def: 4, pv: 8, tags: ["redhair"], preferredRow: "front",
    passive: { name: "Observation de Maître", description: "Vos personnages bénéficient de l'esquive Observation (1x/tour), même avant le tour 5.", effects: [{ type: "grantObservationAll" }] },
    baseAction: { name: "Tir de Précision", atk: 6, description: "Le bras droit de l'Empereur." },
    specialAttack: { name: "Tir de Maître", cost: 3, atkBonus: 3, attackTraits: ["range"], immobilize: true, description: "Portée · la cible perd sa prochaine action." },
  },
  {
    id: "RH-002", name: "Lucky Roux", type: "character", cost: 4,
    faction: "pirate", rarity: "R", set: "ST04",
    atk: 6, def: 3, pv: 8, tags: ["redhair"], preferredRow: "front",
    passive: { name: "Quick Draw", description: "Les attaques de Lucky Roux ne peuvent pas être esquivées.", effects: [{ type: "noDodge" }] },
    baseAction: { name: "Tir", atk: 6, description: "Tir rapide." },
    specialAttack: { name: "Tir à Bout Portant", cost: 2, atkBonus: 3, attackTraits: ["range"], description: "Portée." },
  },
  {
    id: "RH-003", name: "Yasopp", type: "character", cost: 5,
    faction: "pirate", rarity: "R", set: "ST04",
    atk: 5, def: 2, pv: 6, traits: ["range"], tags: ["redhair", "tireur"], preferredRow: "back",
    passive: { name: "Précision Absolue", description: "Les attaques de Yasopp ne peuvent être ni esquivées ni bloquées.", effects: [{ type: "noDodge" }, { type: "attacksIgnoreShield" }] },
    baseAction: { name: "Tir Précis", atk: 5, description: "Il ne rate jamais." },
    specialAttack: { name: "Tir Mortel", cost: 3, atkBonus: 3, attackTraits: ["piercing"], description: "Perçant." },
  },
  {
    id: "RH-004", name: "Rockstar", type: "character", cost: 2,
    faction: "pirate", rarity: "C", set: "ST04",
    atk: 3, def: 2, pv: 4, tags: ["redhair"], preferredRow: "front",
    synergies: [{ partnerId: "CAP-SHANKS", atkBonus: 1 }],
    baseAction: { name: "Coup Insolent", atk: 3, description: "Il défie l'adversaire." },
    specialAttack: { name: "Provocation", cost: 1, atkBonus: 0, isSupport: true, description: "Un ennemi doit cibler Rockstar à son prochain tour." },
  },
  {
    id: "RH-005", name: "Limejuice", type: "character", cost: 3,
    faction: "pirate", rarity: "U", set: "ST04",
    atk: 5, def: 3, pv: 6, tags: ["redhair"], preferredRow: "front",
    naturalHaki: ["armament"],
    passive: { name: "Haki d'Équipage", description: "Haki Armement naturel (attaques touchent les Logia).", effects: [{ type: "naturalHaki", hakiType: "armament" }] },
    baseAction: { name: "Frappe", atk: 5, description: "Coup d'équipage." },
    specialAttack: { name: "Frappe Aguerrie", cost: 2, atkBonus: 3, attackTraits: ["piercing"], description: "Perçant." },
  },
  {
    id: "RH-006", name: "Bonk Punch", type: "character", cost: 3,
    faction: "pirate", rarity: "U", set: "ST04",
    atk: 4, def: 4, pv: 7, traits: ["shield"], tags: ["redhair"], preferredRow: "front",
    baseAction: { name: "Coup de Poing", atk: 4, description: "Le tank." },
    specialAttack: { name: "Charge", cost: 2, atkBonus: 3, attackTraits: ["impact"], description: "Impact." },
  },
  {
    id: "RH-007", name: "Howling Gab", type: "character", cost: 3,
    faction: "pirate", rarity: "C", set: "ST04",
    atk: 3, def: 2, pv: 5, tags: ["redhair"], preferredRow: "front",
    passive: { name: "Hurlement", description: "Un ennemi a -1 ATK tant que Howling Gab est en jeu.", effects: [{ type: "debuffOneEnemy", amount: 1 }] },
    baseAction: { name: "Coup Sonore", atk: 3, description: "Le cri." },
    specialAttack: { name: "Onde de Choc", cost: 2, atkBonus: 2, attackTraits: ["impact"], description: "Impact." },
  },
  {
    id: "RH-008", name: "Building Snake", type: "character", cost: 3,
    faction: "pirate", rarity: "U", set: "ST04",
    atk: 5, def: 3, pv: 6, tags: ["redhair", "bretteur"], preferredRow: "front",
    baseAction: { name: "Coup de Canne", atk: 5, description: "La canne-épée." },
    specialAttack: { name: "Coup Fourbe", cost: 2, atkBonus: 3, attackTraits: ["piercing"], ignoreShield: true, description: "Perçant · ignore le Bouclier." },
  },
  {
    id: "RH-009", name: "Hongo", type: "character", cost: 2,
    faction: "pirate", rarity: "C", set: "ST04",
    atk: 1, def: 2, pv: 4, tags: ["redhair", "medecin"], preferredRow: "back",
    passive: { name: "Médecin de l'Équipage", description: "Début de votre tour : soignez 1 PV à un allié adjacent.", effects: [{ type: "healAdjacent", amount: 1 }] },
    baseAction: { name: "Soins", atk: 0, isSupport: true, healAmount: 2, description: "Soigne 2 PV à un allié." },
    specialAttack: { name: "Stimulant", cost: 2, atkBonus: 0, isSupport: true, description: "Un allié gagne +2 ATK et perd gelé/immobilisé." },
  },

  // === OBJETS - ARMES ===
  {
    id: "RH-010", name: "Gryphon", type: "object", subtype: "weapon", cost: 2,
    faction: "pirate", rarity: "R", set: "ST04", bonusAtk: 2, restriction: "bretteur",
    equipEffect: "+2 ATK. Attaques : Haki Armement (touchent les Logia) et ignorent le Bouclier. Si équipée par Shanks : +1 ATK.",
  },
  {
    id: "RH-011", name: "Fusil de Beckman", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST04", bonusAtk: 1, restriction: "tireur", grantsTraits: ["range"],
    equipEffect: "+1 ATK. Attaques : Portée. Si équipée par Beckman : +1 ATK.",
  },
  {
    id: "RH-012", name: "Pistolet de Lucky Roux", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST04", bonusAtk: 1, restriction: "tireur", grantsTraits: ["range"],
    equipEffect: "+1 ATK. Attaques : Portée. Si équipée par Lucky Roux : attaques inesquivables.",
  },
  {
    id: "RH-013", name: "Fusil de Yasopp", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST04", bonusAtk: 1, restriction: "tireur", grantsTraits: ["range"],
    equipEffect: "+1 ATK. Attaques : Portée. Si équipée par Yasopp : +1 ATK et Perçant.",
  },

  // === OBJETS - ACCESSOIRES ===
  {
    id: "RH-014", name: "Chapeau de Paille", type: "object", subtype: "accessory", cost: 1,
    faction: "pirate", rarity: "R", set: "ST04", bonusAtk: 1,
    equipEffect: "+1 ATK. La première fois que le porteur serait KO, il survit avec 1 PV.",
  },
  {
    id: "RH-015", name: "Sake de la Fête", type: "object", subtype: "accessory", cost: 1,
    faction: "pirate", rarity: "U", set: "ST04", bonusAtk: 0,
    equipEffect: "1x/partie : tous vos alliés sont soignés de 2 PV et gagnent +1 ATK ce tour.",
  },
  {
    id: "RH-016", name: "Cape de l'Empereur", type: "object", subtype: "accessory", cost: 2,
    faction: "pirate", rarity: "R", set: "ST04", bonusAtk: 0, bonusDef: 2,
    equipEffect: "+2 DEF. Les ennemis adjacents au porteur ont -1 ATK.",
  },

  // === NAVIRES ===
  {
    id: "RH-017", name: "Red Force", type: "ship", cost: 2,
    faction: "pirate", rarity: "R", set: "ST04",
    shipPassive: "Vos personnages gagnent +1 ATK.",
    shipActive: { name: "Volonté d'Acier", cost: 3, description: "Ce tour, vos personnages gagnent le Haki Armement et +1 ATK.", oncePerGame: true },
  },
  {
    id: "RH-018", name: "Navire du Nouveau Monde", type: "ship", cost: 1,
    faction: "pirate", rarity: "C", set: "ST04",
    shipPassive: "Vos personnages gagnent +1 PV.",
    shipDestroyEffect: { draw: 1 },
  },

  // === EVENEMENTS ===
  {
    id: "RH-019", name: "Haki du Roi", type: "event", cost: 3,
    faction: "pirate", rarity: "R", set: "ST04",
    eventEffect: { type: "debuffAllEnemies", atk: 2, immobilizeMaxDef: 3 },
  },
  {
    id: "RH-020", name: "Festin", type: "event", cost: 2,
    faction: "pirate", rarity: "U", set: "ST04",
    eventEffect: { type: "healAllBuff", heal: 3, atk: 1 },
  },
  {
    id: "RH-021", name: "Intimidation", type: "event", cost: 2,
    faction: "pirate", rarity: "U", set: "ST04",
    eventEffect: { type: "debuffAllEnemies", atk: 2 },
  },
  {
    id: "RH-022", name: "Promesse", type: "event", cost: 1,
    faction: "pirate", rarity: "C", set: "ST04",
    eventEffect: { type: "buffSingle", stat: "atk", amount: 2, duration: "permanent" },
  },
  {
    id: "RH-023", name: "L'Ère des Rêves", type: "event", cost: 1,
    faction: "pirate", rarity: "C", set: "ST04",
    eventEffect: { type: "grantHakiAll" },
  },
  {
    id: "RH-026", name: "Alliance", type: "event", cost: 2,
    faction: "pirate", rarity: "U", set: "ST04",
    eventEffect: { type: "tutor" },
  },

  // === COUNTERS ===
  {
    id: "RH-024", name: "Courant du Nouveau Monde", type: "counter", cost: 0,
    faction: "pirate", rarity: "C", set: "ST04",
    counterEffect: { type: "reduceDamage", amount: 3 },
  },
  {
    id: "RH-025", name: "Imposer le Respect", type: "counter", cost: 1,
    faction: "pirate", rarity: "U", set: "ST04",
    counterEffect: { type: "reduceDamage", amount: 3 },
  },
  {
    id: "RH-027", name: "Sacrifice du Bras", type: "counter", cost: 3,
    faction: "pirate", rarity: "R", set: "ST04",
    counterEffect: { type: "cancel", description: "Annulez l'attaque ; votre Capitaine subit 5 dégâts.", selfCaptainDamage: 5 },
  },
];

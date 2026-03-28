import type { CardDef } from "@/types";

export const marinesCards: CardDef[] = [
  // === PERSONNAGES ===
  {
    id: "ST02-001", name: "Coby", type: "character", cost: 2,
    faction: "marine", rarity: "C", set: "ST02",
    atk: 2, def: 1, pv: 4,
    traits: ["stealth"], tags: ["marine", "soldat"],
    preferredRow: "back",
    synergies: [{ partnerId: "ST02-002", atkBonus: 1 }],
    baseAction: { name: "Encouragement", atk: 0, isSupport: true, healAmount: 2, description: "+2 PV a 1 allie adjacent." },
    specialAttack: { name: "Soru", cost: 2, atkBonus: 1, description: "Deplace 1 allie vers slot adjacent vide." },
  },
  {
    id: "ST02-002", name: "Helmeppo", type: "character", cost: 2,
    faction: "marine", rarity: "C", set: "ST02",
    atk: 3, def: 2, pv: 4,
    tags: ["marine", "soldat"],
    preferredRow: "front",
    synergies: [{ partnerId: "ST02-001", atkBonus: 1 }],
    baseAction: { name: "Kukri Slash", atk: 3, description: "La lame du fils prodigue." },
    specialAttack: { name: "Justice Slash", cost: 2, atkBonus: 2, description: "Si cible a Prime : +2 bonus." },
  },
  {
    id: "ST02-003", name: "Tashigi", type: "character", cost: 3,
    faction: "marine", rarity: "U", set: "ST02",
    atk: 4, def: 2, pv: 5,
    tags: ["marine", "capitaine_marine", "bretteur"],
    preferredRow: "front",
    baseAction: { name: "Coupe Tranchante", atk: 4, description: "La lame precise de la justice." },
    specialAttack: { name: "Shigure Soen Ryu", cost: 2, atkBonus: 3, description: "Si cible Maudite : +2 bonus." },
  },
  {
    id: "ST02-004", name: "Smoker", type: "character", cost: 4,
    faction: "marine", rarity: "R", set: "ST02",
    atk: 5, def: 3, pv: 7,
    traits: ["range", "cursed", "logia"], tags: ["marine", "capitaine_marine"],
    preferredRow: "front",
    passive: {
      name: "Chasseur Blanc",
      description: "Intangibilite Logia. Quand Smoker attaque : la cible perd Furtif ce tour.",
      effects: [{ type: "logiaIntangibility" }],
    },
    baseAction: { name: "White Blow", atk: 5, attackTraits: ["range"], description: "Immobilise la cible 1 tour." },
    specialAttack: { name: "White Out", cost: 3, atkBonus: 3, attackTraits: ["range", "zone"], description: "Ennemis adjacents perdent Furtif." },
  },
  {
    id: "ST02-005", name: "Sentomaru", type: "character", cost: 3,
    faction: "marine", rarity: "U", set: "ST02",
    atk: 4, def: 4, pv: 7,
    traits: ["shield"], tags: ["marine", "garde"],
    preferredRow: "front",
    passive: {
      name: "Defense Absolue",
      description: "Bouclier. Quand Sentomaru bloque : degats -1. Immunite Impact.",
      effects: [{ type: "immuneImpact" }],
    },
    baseAction: { name: "Sumo Guard", atk: 0, isSupport: true, description: "1 allie adjacent : degats -3 ce tour." },
    specialAttack: { name: "Ashigara Dokkoi", cost: 3, atkBonus: 3, description: "Repousse la cible de 2 slots." },
  },
  {
    id: "ST02-006", name: "Momonga", type: "character", cost: 3,
    faction: "marine", rarity: "U", set: "ST02",
    atk: 5, def: 3, pv: 6,
    tags: ["marine", "vice_amiral", "bretteur"],
    preferredRow: "front",
    naturalHaki: ["armament"],
    passive: {
      name: "Hierarchie : Vice-Amiral",
      description: "Haki Armement naturel. Ses attaques ont le trait Haki.",
      effects: [{ type: "naturalHaki", hakiType: "armament" }],
    },
    baseAction: { name: "Lame du Vice-Amiral", atk: 5, description: "Touche les Logia." },
    specialAttack: { name: "Tranchant Precis", cost: 2, atkBonus: 3, ignoreDef: 1, description: "Ignore 1 DEF." },
  },
  {
    id: "ST02-007", name: "Garp", type: "character", cost: 5,
    faction: "marine", rarity: "SR", set: "ST02",
    atk: 7, def: 4, pv: 10,
    traits: ["shield", "conqueror"], tags: ["marine", "vice_amiral"],
    preferredRow: "front",
    naturalHaki: ["armament"],
    passive: {
      name: "Le Poing du Heros",
      description: "Haki Armement Avance. Bouclier. Immunite Impact.",
      effects: [{ type: "naturalHaki", hakiType: "armament" }, { type: "immuneImpact" }],
    },
    baseAction: { name: "Fist of Love", atk: 7, description: "Le poing d'amour de grand-pere." },
    specialAttack: { name: "Galaxy Impact", cost: 5, atkBonus: 8, attackTraits: ["zone"], oncePerGame: true, ignoreDef: 99, description: "Le poing qui ebranle la terre." },
  },
  {
    id: "ST02-008", name: "Kizaru", type: "character", cost: 5,
    faction: "marine", rarity: "SR", set: "ST02",
    atk: 6, def: 2, pv: 8,
    traits: ["rush", "range", "cursed", "logia"], tags: ["marine", "amiral"],
    preferredRow: "front",
    passive: {
      name: "Lumiere",
      description: "Intangibilite Logia. Rush : attaque le tour du deploiement.",
      effects: [{ type: "logiaIntangibility" }],
    },
    baseAction: { name: "Ama no Murakumo", atk: 6, attackTraits: ["range"], description: "L'epee de lumiere." },
    specialAttack: { name: "Yasakani no Magatama", cost: 4, atkBonus: 5, attackTraits: ["range", "zone"], description: "La pluie de lasers." },
  },
  {
    id: "ST02-009", name: "Aokiji (Kuzan)", type: "character", cost: 5,
    faction: "marine", rarity: "SR", set: "ST02",
    atk: 6, def: 3, pv: 9,
    traits: ["range", "cursed", "logia"], tags: ["marine", "amiral"],
    preferredRow: "front",
    passive: {
      name: "Ere Glaciaire",
      description: "Intangibilite Logia. Immunite Glace.",
      effects: [{ type: "logiaIntangibility" }],
    },
    baseAction: { name: "Ice Block", atk: 0, isSupport: true, immobilize: true, description: "Gele 1 ennemi (perd 1 action)." },
    specialAttack: { name: "Ice Age", cost: 4, atkBonus: 4, element: "ice", attackTraits: ["zone"], description: "Gele tous ennemis Avant." },
  },
  {
    id: "ST02-010", name: "Sengoku", type: "character", cost: 5,
    faction: "marine", rarity: "SR", set: "ST02",
    atk: 6, def: 4, pv: 9,
    traits: ["range", "cursed"], tags: ["marine", "amiral_en_chef"],
    preferredRow: "back",
    passive: {
      name: "Commandant Supreme",
      description: "Tous Marines : couts de deploiement -1 (min 1).",
      effects: [{ type: "costReduction", filter: { faction: "marine" }, amount: 1 }],
    },
    baseAction: { name: "Commandement", atk: 0, isSupport: true, description: "Tous allies Marines +1 ATK ce tour." },
    specialAttack: { name: "Shockwave du Bouddha", cost: 4, atkBonus: 6, attackTraits: ["zone", "total"], oncePerGame: true, description: "L'onde de choc du geant dore." },
  },

  // === OBJETS - FRUITS ===
  {
    id: "ST02-011", name: "Magu Magu no Mi", type: "object", subtype: "fruit", cost: 3,
    faction: "marine", rarity: "SR", set: "ST02",
    bonusAtk: 0, restriction: "Akainu",
    equipEffect: "Maudit, Intangibilite Logia. Attaques gagnent Feu.",
  },
  {
    id: "ST02-012", name: "Moku Moku no Mi", type: "object", subtype: "fruit", cost: 2,
    faction: "marine", rarity: "R", set: "ST02",
    bonusAtk: 0, restriction: "Smoker",
    equipEffect: "Maudit, Intangibilite Logia.",
  },

  // === OBJETS - ARMES ===
  {
    id: "ST02-013", name: "Shigure", type: "object", subtype: "weapon", cost: 1,
    faction: "marine", rarity: "C", set: "ST02",
    bonusAtk: 2, bonusDef: 0, restriction: "bretteur",
    equipEffect: "Si equipee par Tashigi : +1 DEF.",
  },
  {
    id: "ST02-014", name: "Jitte Granit Marin", type: "object", subtype: "weapon", cost: 1,
    faction: "marine", rarity: "U", set: "ST02",
    bonusAtk: 1, restriction: "Smoker",
    equipEffect: "Granit Marin : desactive les pouvoirs de Fruit de la cible pour 1 tour.",
  },

  // === OBJETS - ACCESSOIRES ===
  {
    id: "ST02-015", name: "Menottes G. Marin", type: "object", subtype: "accessory", cost: 2,
    faction: "marine", rarity: "R", set: "ST02",
    bonusAtk: 0,
    equipEffect: "1x/partie : 1 ennemi Maudit perd ses pouvoirs de Fruit 2 tours. ATK -2.",
  },
  {
    id: "ST02-016", name: "Canon Marine", type: "object", subtype: "accessory", cost: 1,
    faction: "marine", rarity: "C", set: "ST02",
    bonusAtk: 0,
    equipEffect: "1x/partie : 3 deg. Portee a 1 cible.",
  },
  {
    id: "ST02-017", name: "Boulet G. Marin", type: "object", subtype: "accessory", cost: 1,
    faction: "marine", rarity: "C", set: "ST02",
    bonusAtk: 0,
    equipEffect: "Usage unique. Attaque gagne Granit Marin ce tour.",
  },

  // === NAVIRES ===
  {
    id: "ST02-018", name: "Navire de Guerre Marine", type: "ship", cost: 2,
    faction: "marine", rarity: "U", set: "ST02",
    shipPassive: "Marines +1 DEF au deploiement. Couts de deploiement Marines -1 (min 1).",
    shipActive: { name: "Bombardement", cost: 3, description: "1x/partie : 3 deg. a toute la Ligne Avant ennemie.", oncePerGame: true },
  },
  {
    id: "ST02-019", name: "Navire de Justice", type: "ship", cost: 1,
    faction: "marine", rarity: "C", set: "ST02",
    shipPassive: "Marines +1 PV au deploiement.",
    shipActive: { name: "Dernier Bastion", cost: 0, description: "Si detruit : tous Marines +2 ATK et +1 DEF ce tour.", oncePerGame: true },
  },

  // === EVENEMENTS ===
  {
    id: "ST02-020", name: "Buster Call", type: "event", cost: 5,
    faction: "marine", rarity: "SR", set: "ST02",
    eventEffect: { type: "damageEnemies", amount: 5, target: "allFront" },
  },
  {
    id: "ST02-021", name: "Promotion", type: "event", cost: 1,
    faction: "marine", rarity: "C", set: "ST02",
    eventEffect: { type: "custom", id: "promotion", description: "1 allie gagne le rang superieur ce tour." },
  },
  {
    id: "ST02-022", name: "Justice Absolue", type: "event", cost: 2,
    faction: "marine", rarity: "U", set: "ST02",
    eventEffect: { type: "buffAllies", stat: "atk", amount: 2, filter: { faction: "marine" }, duration: "turn" },
  },
  {
    id: "ST02-023", name: "Renforts", type: "event", cost: 1,
    faction: "marine", rarity: "C", set: "ST02",
    eventEffect: { type: "draw", amount: 2, discard: 1 },
  },
  {
    id: "ST02-024", name: "Ordre de Tir", type: "event", cost: 2,
    faction: "marine", rarity: "U", set: "ST02",
    eventEffect: { type: "damageEnemies", amount: 3, target: "single" },
  },
  {
    id: "ST02-027", name: "Embargo", type: "event", cost: 2,
    faction: "marine", rarity: "U", set: "ST02",
    eventEffect: { type: "custom", id: "embargo", description: "1 ennemi Maudit ne peut pas utiliser ses pouvoirs ce tour." },
  },
  {
    id: "ST02-028", name: "Execution Publique", type: "event", cost: 3,
    faction: "marine", rarity: "R", set: "ST02",
    eventEffect: { type: "custom", id: "execution", description: "1 ennemi avec <=3 PV est KO. Si Pirate : pioche 2." },
  },

  // === COUNTERS ===
  {
    id: "ST02-025", name: "Manteau de Justice", type: "counter", cost: 0,
    faction: "marine", rarity: "R", set: "ST02",
    counterEffect: { type: "survive", description: "Un allie Marine survit avec 1 PV." },
  },
  {
    id: "ST02-026", name: "Mur d'Acier", type: "counter", cost: 1,
    faction: "marine", rarity: "C", set: "ST02",
    counterEffect: { type: "reduceDamage", amount: 4, captainBonus: 6 },
  },
];

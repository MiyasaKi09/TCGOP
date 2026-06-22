import type { CardDef } from "@/types";

// ST03 — BAROQUE WORKS : UTOPIA — conforme Rulebook v3.1 / ST03_BaroqueWorks_v3.1
export const baroqueCards: CardDef[] = [
  // === PERSONNAGES ===
  {
    id: "BW-001", name: "Mr. 1 (Daz Bonez)", type: "character", cost: 4,
    faction: "pirate", rarity: "R", set: "ST03",
    atk: 6, def: 4, pv: 7, traits: ["piercing"], tags: ["baroque"], preferredRow: "front",
    baseAction: { name: "Lame du Corps", atk: 6, description: "Le corps de lames." },
    specialAttack: { name: "Atomic Spurt", cost: 3, atkBonus: 3, ignoreShield: true, description: "Ignore le Bouclier." },
  },
  {
    id: "BW-002", name: "Miss Doublefinger", type: "character", cost: 3,
    faction: "pirate", rarity: "U", set: "ST03",
    atk: 4, def: 2, pv: 5, tags: ["baroque", "female"], preferredRow: "front",
    passive: { name: "Épines", description: "Quand elle est attaquée en mêlée, l'attaquant subit 2 dégâts.", effects: [{ type: "meleeRecoil", amount: 2 }] },
    baseAction: { name: "Stinger", atk: 4, description: "Les épines jaillissent." },
    specialAttack: { name: "Tsubaki", cost: 2, atkBonus: 3, attackTraits: ["piercing"], description: "Perçant." },
  },
  {
    id: "BW-003", name: "Mr. 2 (Bon Clay)", type: "character", cost: 3,
    faction: "pirate", rarity: "U", set: "ST03",
    atk: 4, def: 2, pv: 6, tags: ["baroque"], preferredRow: "front",
    passive: { name: "Mane Mane", description: "Au déploiement, copie l'ATK d'un de vos autres personnages.", effects: [{ type: "copyAtkOnDeploy" }] },
    baseAction: { name: "Coup de Ballet", atk: 4, description: "La danse okama." },
    specialAttack: { name: "Okama Kenpo", cost: 2, atkBonus: 2, twoTargets: true, description: "Touche 2 cibles." },
  },
  {
    id: "BW-004", name: "Mr. 3 (Galdino)", type: "character", cost: 2,
    faction: "pirate", rarity: "C", set: "ST03",
    atk: 2, def: 3, pv: 5, traits: ["shield"], tags: ["baroque"], preferredRow: "front",
    baseAction: { name: "Coup de Cire", atk: 2, description: "Le mur de cire." },
    specialAttack: { name: "Candle Lock", cost: 2, atkBonus: 2, immobilize: true, description: "La cible est immobilisée 1 tour." },
  },
  {
    id: "BW-005", name: "Miss Goldenweek", type: "character", cost: 2,
    faction: "pirate", rarity: "U", set: "ST03",
    atk: 1, def: 1, pv: 4, tags: ["baroque", "female"], preferredRow: "back",
    passive: { name: "Colors Trap", description: "Un ennemi a -2 ATK tant que Miss Goldenweek est en jeu.", effects: [{ type: "debuffOneEnemy", amount: 2 }] },
    baseAction: { name: "Peinture du Rire", atk: 0, isSupport: true, immobilize: true, description: "Un ennemi perd sa prochaine action (figé de rire)." },
    specialAttack: { name: "Peinture de la Colère", cost: 1, atkBonus: 0, isSupport: true, description: "Un ennemi doit cibler Miss Goldenweek à son prochain tour." },
  },
  {
    id: "BW-006", name: "Mr. 5", type: "character", cost: 2,
    faction: "pirate", rarity: "C", set: "ST03",
    atk: 3, def: 1, pv: 4, tags: ["baroque"], preferredRow: "front",
    passive: { name: "Corps Explosif", description: "Quand Mr. 5 est KO, inflige 2 dégâts à un ennemi adjacent.", effects: [{ type: "explodeOnKO", amount: 2 }] },
    baseAction: { name: "Coup Explosif", atk: 3, description: "Boom." },
    specialAttack: { name: "Nose Fancy Cannon", cost: 2, atkBonus: 2, attackTraits: ["zone"], description: "Zone." },
  },
  {
    id: "BW-007", name: "Miss Valentine", type: "character", cost: 2,
    faction: "pirate", rarity: "C", set: "ST03",
    atk: 3, def: 1, pv: 4, traits: ["rush"], tags: ["baroque", "female"], preferredRow: "front",
    baseAction: { name: "Coup Léger", atk: 3, description: "Elle plane." },
    specialAttack: { name: "Tonne Drop", cost: 2, atkBonus: 3, attackTraits: ["impact"], pushback: true, description: "Impact." },
  },
  {
    id: "BW-008", name: "Mr. 4", type: "character", cost: 3,
    faction: "pirate", rarity: "U", set: "ST03",
    atk: 5, def: 3, pv: 7, tags: ["baroque"], preferredRow: "front",
    passive: { name: "Quatre Tonnes", description: "Ne peut pas être repoussé ni déplacé de force.", effects: [{ type: "immuneImpact" }] },
    baseAction: { name: "Coup de Batte", atk: 5, description: "Le batteur." },
    specialAttack: { name: "Home Run", cost: 3, atkBonus: 4, attackTraits: ["impact"], pushback: true, description: "Impact · repousse." },
  },
  {
    id: "BW-009", name: "Miss Merry Christmas", type: "character", cost: 2,
    faction: "pirate", rarity: "C", set: "ST03",
    atk: 3, def: 2, pv: 5, tags: ["baroque", "female"], preferredRow: "front",
    baseAction: { name: "Coup de Griffe", atk: 3, description: "Elle creuse." },
    specialAttack: { name: "Mole Attack", cost: 2, atkBonus: 2, attackTraits: ["range"], description: "Elle surgit du sol (Portée)." },
  },
  {
    id: "BW-010", name: "Miss All Sunday (Robin)", type: "character", cost: 3,
    faction: "pirate", rarity: "R", set: "ST03",
    atk: 3, def: 2, pv: 5, traits: ["range"], tags: ["baroque", "female"], preferredRow: "back",
    passive: { name: "Vice-Présidente", description: "À l'entrée, l'adversaire défausse une carte au hasard.", effects: [{ type: "entryDiscardRandom" }] },
    baseAction: { name: "Seis Fleur", atk: 3, description: "Des bras surgissent." },
    specialAttack: { name: "Spider Net", cost: 2, atkBonus: 2, immobilize: true, description: "La cible est immobilisée 1 tour." },
  },

  // === OBJETS - ARMES ===
  {
    id: "BW-013", name: "Crochet Empoisonné", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "U", set: "ST03", bonusAtk: 1, grantsElement: "poison",
    equipEffect: "+1 ATK. Attaques du porteur : élément Poison (1 dég./tour permanent).",
  },
  {
    id: "BW-014", name: "Lassoo", type: "object", subtype: "weapon", cost: 1,
    faction: "pirate", rarity: "C", set: "ST03", bonusAtk: 1, restriction: "Mr. 4", grantsElement: "fire",
    equipEffect: "+1 ATK. Attaques : Feu. Si détruite : déployez un jeton.",
  },

  // === OBJETS - FRUITS DU DÉMON ===
  {
    id: "BW-011", name: "Suna Suna no Mi", type: "object", subtype: "fruit", cost: 3,
    faction: "pirate", rarity: "SR", set: "ST03", bonusAtk: 0, restriction: "Crocodile",
    equipEffect: "Applique Logia + Maudit.",
    fruitEffects: {
      base: { grantsTraits: ["cursed", "logia"], passiveDescription: "Logia. Sables : 2 Vol · ATK +3 · Sable (-1 PV permanent)." },
      awakening: {
        porteurLegitime: "Crocodile", minTurns: 5, volCost: 3, atkBonus: 5,
        passiveDescription: "Fin de votre tour : tous les ennemis blessés perdent 1 PV permanent.",
        specialAttack: { name: "Ground Death", cost: 4, atkBonus: 5, oncePerGame: true, element: "sand", description: "La cible perd 3 PV permanent et ne peut plus être soignée." },
      },
    },
  },
  {
    id: "BW-012", name: "Supa Supa no Mi", type: "object", subtype: "fruit", cost: 2,
    faction: "pirate", rarity: "R", set: "ST03", bonusAtk: 0, restriction: "Mr. 1",
    equipEffect: "Applique Perçant + Maudit.",
    fruitEffects: {
      base: { grantsTraits: ["cursed", "piercing"], passiveDescription: "Perçant. Spartan : 1 Vol · ATK +2 · Perçant." },
      awakening: {
        porteurLegitime: "Mr. 1", minTurns: 5, volCost: 2, atkBonus: 0,
        passiveDescription: "Les attaques ignorent le Bouclier et la DEF.",
        specialAttack: { name: "Atomic Spurt", cost: 3, atkBonus: 3, attackTraits: ["total"], ignoreShield: true, ignoreDef: 99, description: "Total · ignore Bouclier et DEF." },
      },
    },
  },

  // === OBJETS - ACCESSOIRES ===
  {
    id: "BW-015", name: "Den Den Mushi Secret", type: "object", subtype: "accessory", cost: 1,
    faction: "pirate", rarity: "C", set: "ST03", bonusAtk: 0,
    equipEffect: "À l'entrée, regardez la main adverse. Les attaques du porteur ignorent le Furtif.",
  },
  {
    id: "BW-016", name: "Bananawani", type: "object", subtype: "accessory", cost: 2,
    faction: "pirate", rarity: "U", set: "ST03", bonusAtk: 0,
    equipEffect: "À l'entrée du porteur, déployez un jeton Bananawani (ATK 4 / DEF 1 / PV 4) adjacent.",
  },
  {
    id: "BW-017", name: "Poudre Explosive", type: "object", subtype: "accessory", cost: 1,
    faction: "pirate", rarity: "C", set: "ST03", bonusAtk: 0,
    equipEffect: "1x/partie : 3 dégâts à un ennemi (Zone).",
  },

  // === NAVIRES ===
  {
    id: "BW-018", name: "Rain Dinners", type: "ship", cost: 1,
    faction: "pirate", rarity: "U", set: "ST03",
    shipPassive: "Vos Baroque Works gagnent +1 ATK.",
    shipActive: { name: "Casino", cost: 1, description: "Piochez 1 carte puis défaussez 1 carte." },
  },
  {
    id: "BW-019", name: "Navire Baroque Works", type: "ship", cost: 2,
    faction: "pirate", rarity: "U", set: "ST03",
    shipPassive: "Vos Baroque Works gagnent +1 PV.",
    shipActive: { name: "Agents", cost: 2, description: "Déployez deux jetons agents.", oncePerGame: true },
  },

  // === EVENEMENTS ===
  {
    id: "BW-020", name: "Operation Utopia", type: "event", cost: 3,
    faction: "pirate", rarity: "R", set: "ST03",
    eventEffect: { type: "damageEnemies", amount: 3, target: "all" },
  },
  {
    id: "BW-021", name: "Embuscade", type: "event", cost: 2,
    faction: "pirate", rarity: "U", set: "ST03",
    eventEffect: { type: "rushBuff", atk: 2 },
  },
  {
    id: "BW-022", name: "Contrat d'Assassinat", type: "event", cost: 1,
    faction: "pirate", rarity: "C", set: "ST03",
    eventEffect: { type: "custom", id: "execute3", description: "Détruisez un ennemi de PV actuels ≤ 3." },
  },
  {
    id: "BW-023", name: "Tempête de Sable", type: "event", cost: 3,
    faction: "pirate", rarity: "U", set: "ST03",
    eventEffect: { type: "damageEnemies", amount: 2, target: "all", sand: true },
  },
  {
    id: "BW-024", name: "Trahison", type: "event", cost: 2,
    faction: "pirate", rarity: "U", set: "ST03",
    eventEffect: { type: "custom", id: "betrayal", description: "Prenez le contrôle d'un ennemi de coût ≤ 2 ce tour." },
  },
  {
    id: "BW-027", name: "Infiltration", type: "event", cost: 1,
    faction: "pirate", rarity: "C", set: "ST03",
    eventEffect: { type: "tutor", filterTag: "baroque", maxCost: 3 },
  },
  {
    id: "BW-028", name: "Pluie Artificielle", type: "event", cost: 2,
    faction: "pirate", rarity: "U", set: "ST03",
    eventEffect: { type: "custom", id: "noHeal2", description: "Persistant (2 tours) : les ennemis ne peuvent pas être soignés." },
  },

  // === COUNTERS ===
  {
    id: "BW-025", name: "« Faible »", type: "counter", cost: 0,
    faction: "pirate", rarity: "C", set: "ST03",
    counterEffect: { type: "cancel", description: "Annulez une attaque d'un ennemi d'ATK ≤ 4.", maxAttackerAtk: 4 },
  },
  {
    id: "BW-026", name: "Mirage du Désert", type: "counter", cost: 1,
    faction: "pirate", rarity: "C", set: "ST03",
    counterEffect: { type: "untargetable", description: "La cible devient Inciblable jusqu'à la fin du tour." },
  },
];

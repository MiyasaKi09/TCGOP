// ============================================================
// ONE PIECE GRAND LINE TCG — Core Type Definitions
// ============================================================

// --- Enums & Literal Types ---

export type PlayerId = "player1" | "player2";

export type CardType = "character" | "object" | "ship" | "event" | "counter";

export type ObjectSubtype = "weapon" | "fruit" | "accessory";

export type Element = "fire" | "water" | "thunder" | "ice" | "sand" | "poison";

export type Trait =
  | "shield"     // S'incline pour bloquer pour 1 adjacent
  | "range"      // Attaque depuis l'Arriere, cible libre
  | "stealth"    // Inciblable tant qu'un allie non-Furtif existe
  | "rush"       // Ignore le mal de terre (attaques seulement)
  | "cursed"     // Porteur de Fruit du Demon, vulnerable Eau/Granit
  | "logia"      // Intangibilite: ignore degats sans Haki/Eau/Granit
  | "piercing"   // Toutes les attaques divisent DEF par 2
  | "conqueror"; // Acces au Haki du Roi (T10+)

export type AttackTrait = "range" | "piercing" | "zone" | "total";

export type HakiType = "observation" | "armament" | "king";

export type Slot = "V1" | "V2" | "V3" | "A1" | "A2" | "A3";

export type Row = "front" | "back";

export type Phase = "untap" | "draw" | "willGain" | "main" | "end";

export type Faction = "pirate" | "marine" | "revolutionary" | "independent";

// --- Card Definitions (static data, from card catalogue) ---

export interface BaseAction {
  name: string;
  /** ATK value for attacks, 0 for support actions */
  atk: number;
  /** Trait keywords on this specific attack */
  attackTraits?: AttackTrait[];
  element?: Element;
  /** Is this a support action (heal, buff, etc.) rather than an attack? */
  isSupport?: boolean;
  /** Heal amount if support */
  healAmount?: number;
  /** Control effect: immobilize target */
  immobilize?: boolean;
  /** Description text */
  description?: string;
}

export interface SpecialAttack {
  name: string;
  /** Volonte cost */
  cost: number;
  /** ATK bonus added to base ATK. Total = baseATK + atkBonus */
  atkBonus: number;
  attackTraits?: AttackTrait[];
  element?: Element;
  /** 1x per game */
  oncePerGame?: boolean;
  /** Ignore N points of DEF */
  ignoreDef?: number;
  /** Is support (heal, buff) rather than damage */
  isSupport?: boolean;
  healAmount?: number;
  description?: string;
}

export interface SynergyDef {
  /** Card ID of the synergy partner */
  partnerId: string;
  /** Bonus ATK when partner is in play */
  atkBonus: number;
  /** Bonus ATK when partner is KO (rage) — this turn only */
  onPartnerKO?: number;
}

export interface PassiveDef {
  name: string;
  description: string;
  /** Encoded effects — kept as structured data for the engine */
  effects: PassiveEffect[];
}

export type PassiveEffect =
  | { type: "buffAlly"; stat: "atk" | "def" | "pv"; amount: number; filter?: AllyFilter }
  | { type: "healAdjacent"; amount: number }
  | { type: "onAllyKO"; effect: "bonusWill"; amount: number }
  | { type: "naturalHaki"; hakiType: HakiType }
  | { type: "threeWeaponSlots" }
  | { type: "twoAccessorySlots" }
  | { type: "cannotAttackFemale" }
  | { type: "revive"; pv: number }
  | { type: "logiaIntangibility" }
  | { type: "immuneImpact" }
  | { type: "meleeRecoil"; amount: number }
  | { type: "costReduction"; filter?: AllyFilter; amount: number }
  | { type: "hakiCostReduction"; amount: number }
  | { type: "ignoreEnemyStealth" }
  | { type: "custom"; id: string };

export interface AllyFilter {
  faction?: Faction;
  tag?: string;  // e.g. "mugiwara", "marine", "bretteur"
  trait?: Trait;
}

export interface CardDef {
  id: string;
  name: string;
  type: CardType;
  cost: number;
  faction: Faction;
  rarity: "C" | "U" | "R" | "SR" | "L" | "CAP";
  set: string;

  // --- Character fields ---
  atk?: number;
  def?: number;
  pv?: number;
  traits?: Trait[];
  /** Preferred line: front or back (for AI placement) */
  preferredRow?: Row;
  /** Tags for synergy/filter matching */
  tags?: string[];
  /** Does this character have natural Haki? */
  naturalHaki?: HakiType[];
  baseAction?: BaseAction;
  specialAttack?: SpecialAttack;
  passive?: PassiveDef;
  synergies?: SynergyDef[];

  // --- Object fields ---
  subtype?: ObjectSubtype;
  /** ATK bonus when equipped */
  bonusAtk?: number;
  /** DEF bonus when equipped */
  bonusDef?: number;
  /** Restricted to a specific character name or tag */
  restriction?: string;
  /** Equipment grants traits */
  grantsTraits?: (Trait | AttackTrait)[];
  /** Equipment grants an element to attacks */
  grantsElement?: Element;
  /** Special equipment effect description */
  equipEffect?: string;
  /** Devil Fruit structured effects */
  fruitEffects?: {
    base: {
      grantsTraits?: Trait[];
      passiveDescription?: string;
      atkBonus?: number;
      defBonus?: number;
    };
    awakening?: {
      porteurLegitime: string; // character name that can awaken
      minTurns: number;
      volCost: number;
      grantsTraits?: Trait[];
      atkBonus?: number;
      defBonus?: number;
      passiveDescription?: string;
      specialAttack?: { name: string; cost: number; atkBonus: number; description: string; oncePerGame?: boolean };
    };
  };
  /** Is this fruit awakened? (runtime, set on CardInstance not CardDef) */

  // --- Ship fields ---
  shipPassive?: string;
  shipActive?: {
    name: string;
    cost: number;
    description: string;
    oncePerGame?: boolean;
  };

  // --- Event fields ---
  eventEffect?: EventEffect;

  // --- Counter fields ---
  counterEffect?: CounterEffect;
}

// --- Event Effects ---

export type EventEffect =
  | { type: "gainWill"; amount: number }
  | { type: "draw"; amount: number; discard?: number }
  | { type: "healAlly"; amount: number; allAllies?: boolean }
  | { type: "buffAllies"; stat: "atk" | "def"; amount: number; filter?: AllyFilter; duration: "turn" | "permanent" }
  | { type: "damageEnemies"; amount: number; target: "allFront" | "allCursed" | "single" }
  | { type: "dodgeAll" }
  | { type: "custom"; id: string; description: string };

export type CounterEffect =
  | { type: "survive"; description: string }
  | { type: "reduceDamage"; amount: number; captainBonus?: number };

// --- Captain Definition ---

export interface CaptainDef {
  id: string;
  name: string;
  faction: Faction;
  tags?: string[];
  traits?: Trait[];

  recto: {
    pv: number;
    atk: number;
    def: number;
    passive: PassiveDef;
    /** Captain attacks (cost Vol., no free base action) */
    attacks: SpecialAttack[];
    /** Surcharge ability */
    surcharge?: SpecialAttack;
  };

  /** Conditions to flip recto -> verso */
  flipCondition: {
    /** Volonte cost to flip */
    cost?: number;
    /** Auto-flip if allies count <= this */
    autoIfAlliesLte?: number;
  };

  verso: {
    pv: number;
    atk: number;
    def: number;
    passive: PassiveDef;
    entryEffect: EntryEffect;
    baseAction: BaseAction;
    specialAttack: SpecialAttack;
    surcharge?: SpecialAttack;
    traits?: Trait[];
    naturalHaki?: HakiType[];
  };
}

export type EntryEffect =
  | { type: "buffAllies"; stat: "atk" | "def"; amount: number; duration: "turn" }
  | { type: "draw"; amount: number }
  | { type: "damageEnemies"; amount: number; target: "allFront" | "single" }
  | { type: "multi"; effects: EntryEffect[] }
  | { type: "custom"; id: string; description: string };

// --- Deck Definition ---

export interface DeckEntry {
  cardId: string;
  count: number;
}

export interface DeckDef {
  name: string;
  captainId: string;
  cards: DeckEntry[];
}

// --- Runtime Game State ---

export interface Modifier {
  id: string;
  stat: "atk" | "def" | "pv" | "haki";
  amount: number;
  source: string;
  duration: "permanent" | "turn" | "nextTurn";
  turnsRemaining?: number;
}

export interface StatusEffect {
  type: "burn" | "poison" | "freeze" | "desiccation" | "trap" | "immobilize";
  turnsRemaining: number;  // -1 = permanent (poison)
  damagePerTurn: number;
  source: string;
}

export interface CardInstance {
  instanceId: string;
  defId: string;
  owner: PlayerId;
  zone: "deck" | "hand" | "board" | "graveyard" | "banished";
  slot?: Slot;
  tapped: boolean;
  currentPv: number;
  /** Object instanceIds attached to this character */
  attachedObjects: string[];
  modifiers: Modifier[];
  statusEffects: StatusEffect[];
  /** Turn this card was deployed (for summoning sickness) */
  deployedTurn?: number;
  /** Has used base action this turn */
  usedBaseAction: boolean;
  /** Has used special attack this turn */
  usedSpecialAttack: boolean;
  /** Logia: has already ignored damage this turn */
  logiaUsedThisTurn?: boolean;
  /** 1x/game abilities already used */
  usedOnceAbilities: string[];
  /** Is this a Devil Fruit that has been awakened? */
  isAwakened?: boolean;
}

export interface CaptainInstance {
  defId: string;
  owner: PlayerId;
  flipped: boolean;
  currentPv: number;
  slot?: Slot;
  tapped: boolean;
  modifiers: Modifier[];
  statusEffects: StatusEffect[];
  deployedTurn?: number;
  usedBaseAction: boolean;
  usedSpecialAttack: boolean;
  usedOnceAbilities: string[];
}

export interface PlayerState {
  id: PlayerId;
  captain: CaptainInstance;
  deck: string[];       // instanceIds (top = index 0)
  hand: string[];       // instanceIds
  graveyard: string[];  // instanceIds
  board: Record<Slot, string | null>;
  /** Active ship instanceId (max 1) */
  activeShip: string | null;
  volonte: number;
  /** Has used free move this turn */
  usedFreeMove: boolean;
  /** Has drawn this turn */
  hasDrawn: boolean;
  /** Haki: observation used this turn */
  observationUsed: boolean;
  /** Haki: armament used this turn */
  armamentUsed: boolean;
  /** Haki du Roi used this game */
  kingUsed: boolean;
}

export interface PendingAttack {
  attackerId: string;
  targetId: string;
  /** Is this targeting the captain? */
  targetIsCaptain: boolean;
  /** Is this a special attack? */
  isSpecial: boolean;
  /** Calculated raw damage before counter */
  rawDamage: number;
  /** Attack element if any */
  element?: Element;
  /** Attack traits */
  attackTraits: AttackTrait[];
  /** Does this attack have haki? */
  hasHaki: boolean;
}

export interface LogEntry {
  turn: number;
  player: PlayerId;
  message: string;
}

export interface GameState {
  /** All card instances by instanceId */
  cards: Record<string, CardInstance>;
  players: Record<PlayerId, PlayerState>;
  turnNumber: number;
  /** Whose turn is it? (overall turn count — T1 = player1, T2 = player2, etc.) */
  currentPlayer: PlayerId;
  phase: Phase;
  /** Pending attack waiting for counter response */
  pendingAttack: PendingAttack | null;
  log: LogEntry[];
  winner: PlayerId | null;
  /** Turn of first player (alternates) */
  firstPlayer: PlayerId;
}

// --- Game Actions (discriminated union) ---

export type GameAction =
  | { type: "deployCharacter"; instanceId: string; slot: Slot }
  | { type: "equipObject"; objectInstanceId: string; targetInstanceId: string }
  | { type: "deployShip"; instanceId: string }
  | { type: "baseAttack"; attackerInstanceId: string; targetInstanceId: string; targetIsCaptain?: boolean }
  | { type: "specialAttack"; attackerInstanceId: string; targetInstanceId: string; targetIsCaptain?: boolean }
  | { type: "baseSupportAction"; instanceId: string; targetInstanceId?: string }
  | { type: "playEvent"; instanceId: string; targets?: string[] }
  | { type: "playCounter"; instanceId: string }
  | { type: "passCounter" }
  | { type: "flipCaptain"; slot: Slot }
  | { type: "captainAttack"; targetInstanceId: string; targetIsCaptain?: boolean; isSpecial?: boolean }
  | { type: "useHaki"; hakiType: HakiType; targetInstanceId?: string }
  | { type: "moveCharacter"; instanceId: string; targetSlot: Slot }
  | { type: "activateShip"; shipInstanceId: string }
  | { type: "awakenFruit"; fruitInstanceId: string }
  | { type: "fruitSpecialAttack"; attackerInstanceId: string; fruitInstanceId: string; targetInstanceId: string; targetIsCaptain?: boolean }
  | { type: "endTurn" };

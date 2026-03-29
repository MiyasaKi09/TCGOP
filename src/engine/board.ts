import { produce } from "immer";
import type {
  GameState,
  PlayerId,
  Slot,
  CardInstance,
  Trait,
} from "@/types";
import { getCardDef } from "./cardRegistry";
import { spendVolonte, canAfford } from "./volonte";
import { addLog, getOpponent } from "./gameState";
import { ADJACENCY, FRONT_SLOTS, BACK_SLOTS, ALL_SLOTS } from "./utils";

// ============================================================
// Queries
// ============================================================

/** Get all characters on the board for a player */
export function getBoardCharacters(
  state: GameState,
  playerId: PlayerId
): CardInstance[] {
  const player = state.players[playerId];
  const result: CardInstance[] = [];
  for (const slot of ALL_SLOTS) {
    const id = player.board[slot];
    if (id) {
      const card = state.cards[id];
      if (card) result.push(card);
    }
  }
  return result;
}

/** Get character in a specific slot */
export function getCharacterInSlot(
  state: GameState,
  playerId: PlayerId,
  slot: Slot
): CardInstance | null {
  const id = state.players[playerId].board[slot];
  return id ? state.cards[id] ?? null : null;
}

/** Get empty slots for a player */
export function getEmptySlots(
  state: GameState,
  playerId: PlayerId
): Slot[] {
  const player = state.players[playerId];
  return ALL_SLOTS.filter((s) => player.board[s] === null) as Slot[];
}

/** Get the slot of a card instance on the board */
export function getSlotOf(
  state: GameState,
  instanceId: string
): Slot | null {
  const card = state.cards[instanceId];
  if (!card || card.zone !== "board") return null;
  return card.slot ?? null;
}

/** Check if a slot is in the front row */
export function isFrontSlot(slot: Slot): boolean {
  return (FRONT_SLOTS as readonly string[]).includes(slot);
}

/** Check if a slot is in the back row */
export function isBackSlot(slot: Slot): boolean {
  return (BACK_SLOTS as readonly string[]).includes(slot);
}

/** Get adjacent slots */
export function getAdjacentSlots(slot: Slot): Slot[] {
  return (ADJACENCY[slot] ?? []) as Slot[];
}

/** Check if a player has any front-row characters */
export function hasFrontRow(
  state: GameState,
  playerId: PlayerId
): boolean {
  const player = state.players[playerId];
  return FRONT_SLOTS.some((s) => player.board[s] !== null);
}

/** Get the effective ATK of a character (base + equipment + modifiers) */
export function getEffectiveAtk(
  state: GameState,
  instanceId: string
): number {
  const card = state.cards[instanceId];
  if (!card) return 0;
  const def = getCardDef(card.defId);
  let atk = def.atk ?? 0;

  // Equipment bonuses
  for (const objId of card.attachedObjects) {
    const objCard = state.cards[objId];
    if (objCard) {
      const objDef = getCardDef(objCard.defId);
      atk += objDef.bonusAtk ?? 0;
    }
  }

  // Modifier bonuses
  for (const mod of card.modifiers) {
    if (mod.stat === "atk") atk += mod.amount;
  }

  return Math.max(0, atk);
}

/** Get the effective DEF of a character */
export function getEffectiveDef(
  state: GameState,
  instanceId: string
): number {
  const card = state.cards[instanceId];
  if (!card) return 0;
  const def = getCardDef(card.defId);
  let defVal = def.def ?? 0;

  // Equipment bonuses
  for (const objId of card.attachedObjects) {
    const objCard = state.cards[objId];
    if (objCard) {
      const objDef = getCardDef(objCard.defId);
      defVal += objDef.bonusDef ?? 0;
    }
  }

  // Modifier bonuses
  for (const mod of card.modifiers) {
    if (mod.stat === "def") defVal += mod.amount;
  }

  return Math.max(0, defVal);
}

/** Check if a character has a specific trait */
export function hasTrait(
  state: GameState,
  instanceId: string,
  trait: Trait
): boolean {
  const card = state.cards[instanceId];
  if (!card) return false;
  const def = getCardDef(card.defId);
  return def.traits?.includes(trait) ?? false;
}

/** Check if character has summoning sickness (deployed this turn, no Rush) */
export function hasSummoningSickness(
  state: GameState,
  instanceId: string
): boolean {
  const card = state.cards[instanceId];
  if (!card) return false;
  if (card.deployedTurn === state.turnNumber) {
    return !hasTrait(state, instanceId, "rush");
  }
  return false;
}

// ============================================================
// Mutations
// ============================================================

/**
 * Deploy a character from hand to a board slot.
 * Pays the Volonte cost. Marks summoning sickness.
 */
export function deployCharacter(
  state: GameState,
  playerId: PlayerId,
  instanceId: string,
  slot: Slot
): GameState {
  const card = state.cards[instanceId];
  if (!card) throw new Error(`Card not found: ${instanceId}`);
  if (card.owner !== playerId) throw new Error("Not your card");
  if (card.zone !== "hand") throw new Error("Card not in hand");

  const def = getCardDef(card.defId);
  if (def.type !== "character") throw new Error("Not a character card");

  const player = state.players[playerId];
  if (player.board[slot] !== null) throw new Error(`Slot ${slot} is occupied`);

  if (!canAfford(state, playerId, def.cost)) {
    throw new Error(`Cannot afford ${def.name} (cost ${def.cost})`);
  }

  let next = spendVolonte(state, playerId, def.cost);

  next = produce(next, (draft) => {
    const p = draft.players[playerId];
    const c = draft.cards[instanceId];

    // Remove from hand
    p.hand = p.hand.filter((id) => id !== instanceId);

    // Place on board
    p.board[slot] = instanceId;
    c.zone = "board";
    c.slot = slot;
    c.deployedTurn = draft.turnNumber;
    c.currentPv = def.pv ?? 0;
  });

  next = addLog(next, playerId, `Deploie ${def.name} en ${slot}`);

  // Recalculate passive buffs (new character on board may trigger synergies, captain buffs)
  const { recalculatePassiveBuffs } = require("./passives");
  next = recalculatePassiveBuffs(next, playerId);

  return next;
}

/**
 * Equip an object from hand onto a character on the board.
 */
export function equipObject(
  state: GameState,
  playerId: PlayerId,
  objectInstanceId: string,
  targetInstanceId: string
): GameState {
  const objCard = state.cards[objectInstanceId];
  if (!objCard) throw new Error(`Object not found: ${objectInstanceId}`);
  if (objCard.owner !== playerId) throw new Error("Not your card");
  if (objCard.zone !== "hand") throw new Error("Object not in hand");

  const objDef = getCardDef(objCard.defId);
  if (objDef.type !== "object") throw new Error("Not an object card");

  const targetCard = state.cards[targetInstanceId];
  if (!targetCard) throw new Error(`Target not found: ${targetInstanceId}`);
  if (targetCard.owner !== playerId) throw new Error("Not your character");
  if (targetCard.zone !== "board") throw new Error("Target not on board");

  if (!canAfford(state, playerId, objDef.cost)) {
    throw new Error(`Cannot afford ${objDef.name} (cost ${objDef.cost})`);
  }

  // Check equipment slot limits (simplified — 1 weapon, 1 fruit, 1 accessory)
  const targetDef = getCardDef(targetCard.defId);
  const existingObjects = targetCard.attachedObjects.map(
    (id) => getCardDef(state.cards[id].defId)
  );

  if (objDef.subtype) {
    const sameSubtype = existingObjects.filter(
      (d) => d.subtype === objDef.subtype
    );
    // Check for exceptions (Zoro 3 weapons, Franky 2 accessories)
    let maxSlots = 1;
    const passiveEffects = targetDef.passive?.effects ?? [];
    if (objDef.subtype === "weapon") {
      if (passiveEffects.some((e) => e.type === "threeWeaponSlots")) {
        maxSlots = 3;
      }
    }
    if (objDef.subtype === "accessory") {
      if (passiveEffects.some((e) => e.type === "twoAccessorySlots")) {
        maxSlots = 2;
      }
    }
    if (sameSubtype.length >= maxSlots) {
      throw new Error(
        `${targetDef.name} already has max ${objDef.subtype} equipped`
      );
    }
  }

  let next = spendVolonte(state, playerId, objDef.cost);

  next = produce(next, (draft) => {
    const p = draft.players[playerId];
    const obj = draft.cards[objectInstanceId];
    const target = draft.cards[targetInstanceId];

    // Remove from hand
    p.hand = p.hand.filter((id) => id !== objectInstanceId);

    // Attach to character
    obj.zone = "board";
    obj.slot = target.slot;
    target.attachedObjects.push(objectInstanceId);
  });

  next = addLog(
    next,
    playerId,
    `Equipe ${objDef.name} sur ${getCardDef(targetCard.defId).name}`
  );
  return next;
}

/**
 * Deploy a ship (max 1 active, replaces previous).
 */
export function deployShip(
  state: GameState,
  playerId: PlayerId,
  instanceId: string
): GameState {
  const card = state.cards[instanceId];
  if (!card) throw new Error(`Card not found: ${instanceId}`);
  if (card.owner !== playerId) throw new Error("Not your card");
  if (card.zone !== "hand") throw new Error("Card not in hand");

  const def = getCardDef(card.defId);
  if (def.type !== "ship") throw new Error("Not a ship card");

  if (!canAfford(state, playerId, def.cost)) {
    throw new Error(`Cannot afford ${def.name} (cost ${def.cost})`);
  }

  let next = spendVolonte(state, playerId, def.cost);

  next = produce(next, (draft) => {
    const p = draft.players[playerId];

    // Discard previous ship if any
    if (p.activeShip) {
      const oldShip = draft.cards[p.activeShip];
      if (oldShip) {
        oldShip.zone = "graveyard";
        p.graveyard.push(p.activeShip);
      }
    }

    // Remove from hand
    p.hand = p.hand.filter((id) => id !== instanceId);

    // Set as active ship
    p.activeShip = instanceId;
    draft.cards[instanceId].zone = "board";
  });

  next = addLog(next, playerId, `Deploie navire ${def.name}`);
  return next;
}

/**
 * Move a character to an adjacent empty slot (free move, 1x/turn).
 */
export function moveCharacter(
  state: GameState,
  playerId: PlayerId,
  instanceId: string,
  targetSlot: Slot
): GameState {
  const player = state.players[playerId];
  if (player.usedFreeMove) throw new Error("Free move already used this turn");

  const card = state.cards[instanceId];
  if (!card || card.zone !== "board") throw new Error("Card not on board");
  if (card.owner !== playerId) throw new Error("Not your card");

  const currentSlot = card.slot;
  if (!currentSlot) throw new Error("Card has no slot");

  const adjacent = getAdjacentSlots(currentSlot);
  if (!adjacent.includes(targetSlot)) {
    throw new Error(`${targetSlot} is not adjacent to ${currentSlot}`);
  }

  if (player.board[targetSlot] !== null) {
    throw new Error(`Slot ${targetSlot} is occupied`);
  }

  return produce(state, (draft) => {
    const p = draft.players[playerId];
    const c = draft.cards[instanceId];

    p.board[currentSlot] = null;
    p.board[targetSlot] = instanceId;
    c.slot = targetSlot;
    p.usedFreeMove = true;
  });
}

/**
 * Remove a character from the board (KO'd).
 * Sends character and attached objects to graveyard.
 */
export function removeFromBoard(
  state: GameState,
  instanceId: string
): GameState {
  const card = state.cards[instanceId];
  if (!card || card.zone !== "board") return state;

  return produce(state, (draft) => {
    const c = draft.cards[instanceId];
    const player = draft.players[c.owner];
    const slot = c.slot;

    if (slot) {
      player.board[slot] = null;
    }

    // Move attached objects to graveyard
    for (const objId of c.attachedObjects) {
      const obj = draft.cards[objId];
      if (obj) {
        obj.zone = "graveyard";
        player.graveyard.push(objId);
      }
    }
    c.attachedObjects = [];

    // Move character to graveyard
    c.zone = "graveyard";
    c.slot = undefined;
    player.graveyard.push(instanceId);
  });
}

// ============================================================
// Valid targets for attacks
// ============================================================

/**
 * Get valid attack targets for an attacker.
 * Rules:
 * - Front attacker → any enemy Front
 * - Front attacker with Range → any enemy
 * - Back attacker without Range → cannot melee attack
 * - Back attacker with Range → any enemy
 * - If no enemy Front → can target Back and Captain
 * - Stealth: can't be targeted while non-Stealth ally exists
 * - Captain (verso, on board) → targetable like a normal character
 * - Captain (recto, off board) → targetable if no enemy Front
 */
export function getValidTargets(
  state: GameState,
  attackerInstanceId: string,
  forSpecial?: boolean
): { characterTargets: string[]; canTargetCaptain: boolean } {
  const attacker = state.cards[attackerInstanceId];
  if (!attacker) return { characterTargets: [], canTargetCaptain: false };

  const attackerDef = getCardDef(attacker.defId);
  const attackerSlot = attacker.slot;
  if (!attackerSlot) return { characterTargets: [], canTargetCaptain: false };

  const opponentId = getOpponent(attacker.owner);
  const opponent = state.players[opponentId];

  // Check range from character trait OR from the specific attack's traits
  let hasRange = attackerDef.traits?.includes("range") ?? false;
  if (!hasRange && forSpecial && attackerDef.specialAttack?.attackTraits?.includes("range")) {
    hasRange = true;
  }
  if (!hasRange && !forSpecial && attackerDef.baseAction?.attackTraits?.includes("range")) {
    hasRange = true;
  }

  const attackerInBack = isBackSlot(attackerSlot);

  // Back row without Range can't melee attack
  if (attackerInBack && !hasRange) {
    return { characterTargets: [], canTargetCaptain: false };
  }

  const opponentHasFront = hasFrontRow(state, opponentId);
  const opponentChars = getBoardCharacters(state, opponentId);

  // Determine targetable characters
  let targetable = opponentChars;

  if (opponentHasFront && !hasRange) {
    // Can only target front row
    targetable = targetable.filter(
      (c) => c.slot && isFrontSlot(c.slot)
    );
  }

  // Apply Stealth filter
  const hasNonStealth = targetable.some(
    (c) => !hasTrait(state, c.instanceId, "stealth")
  );
  if (hasNonStealth) {
    targetable = targetable.filter(
      (c) => !hasTrait(state, c.instanceId, "stealth")
    );
  }

  // Can target captain?
  let canTargetCaptain = false;
  if (opponent.captain.flipped && opponent.captain.slot) {
    // Verso captain is on board — targetable like a character
    // (subject to front row protection)
    if (!opponentHasFront || hasRange) {
      canTargetCaptain = true;
    } else if (isFrontSlot(opponent.captain.slot)) {
      canTargetCaptain = true;
    }
  } else {
    // Recto captain — targetable only if no front row
    if (!opponentHasFront || hasRange) {
      canTargetCaptain = true;
    }
  }

  return {
    characterTargets: targetable.map((c) => c.instanceId),
    canTargetCaptain,
  };
}

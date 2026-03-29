import { produce } from "immer";
import type {
  GameState,
  PlayerId,
  PendingAttack,
  AttackTrait,
  Element,
  CardInstance,
} from "@/types";
import { getCardDef, getCaptainDef } from "./cardRegistry";
import {
  getEffectiveAtk,
  getEffectiveDef,
  hasTrait,
  hasSummoningSickness,
  removeFromBoard,
  isFrontSlot,
  getAdjacentSlots,
  getBoardCharacters,
} from "./board";
import { spendVolonte, canAfford, grantKOBonus } from "./volonte";
import { addLog, getOpponent, checkWinCondition } from "./gameState";

// ============================================================
// Step 1: Declare Attack
// ============================================================

/**
 * Declare a base attack (free, taps the attacker).
 */
export function declareBaseAttack(
  state: GameState,
  attackerInstanceId: string,
  targetInstanceId: string,
  targetIsCaptain: boolean
): GameState {
  const attacker = state.cards[attackerInstanceId];
  if (!attacker) throw new Error("Attacker not found");
  if (attacker.tapped) throw new Error("Attacker is tapped");
  if (attacker.usedBaseAction) throw new Error("Base action already used");
  if (hasSummoningSickness(state, attackerInstanceId)) {
    throw new Error("Character has summoning sickness");
  }

  const def = getCardDef(attacker.defId);
  if (getEffectiveAtk(state, attackerInstanceId) <= 0) {
    throw new Error("Character has 0 ATK — cannot attack");
  }

  // Trigger trap on attacker if present
  const trapEffect = attacker.statusEffects.find((e) => e.type === "trap");
  if (trapEffect) {
    state = produce(state, (draft) => {
      const a = draft.cards[attackerInstanceId];
      a.currentPv -= trapEffect.damagePerTurn;
      a.statusEffects = a.statusEffects.filter((e) => e.type !== "trap");
    });
    state = addLog(
      state,
      attacker.owner,
      `Piege ! ${def.name} subit ${trapEffect.damagePerTurn} degats en attaquant !`
    );
    // Check if attacker is KO'd by trap
    if (state.cards[attackerInstanceId].currentPv <= 0) {
      state = addLog(state, attacker.owner, `${def.name} est KO par le piege !`);
      state = removeFromBoard(state, attackerInstanceId);
      return state;
    }
  }

  const baseAction = def.baseAction;
  const atk = getEffectiveAtk(state, attackerInstanceId);
  const attackTraits: AttackTrait[] = baseAction?.attackTraits ?? [];

  // Check equipped objects for granted element (e.g. Baril d'Eau grants "water")
  let attackElement = baseAction?.element;
  for (const objId of attacker.attachedObjects) {
    const objCard = state.cards[objId];
    if (objCard) {
      const objDef = getCardDef(objCard.defId);
      if (objDef.grantsElement) {
        attackElement = objDef.grantsElement;
      }
    }
  }

  // Calculate raw damage against target
  let targetDef = 0;
  if (targetIsCaptain) {
    const opponent = getOpponent(attacker.owner);
    const cap = state.players[opponent].captain;
    const capDef = getCaptainDef(cap.defId);
    targetDef = cap.flipped ? capDef.verso.def : capDef.recto.def;
    // Apply captain modifiers
    for (const mod of cap.modifiers) {
      if (mod.stat === "def") targetDef += mod.amount;
    }
  } else {
    targetDef = getEffectiveDef(state, targetInstanceId);
  }

  // Apply Piercing (DEF / 2)
  const isPiercing =
    attackTraits.includes("piercing") ||
    (def.traits?.includes("piercing") ?? false);
  if (isPiercing) {
    targetDef = Math.floor(targetDef / 2);
  }

  const rawDamage = Math.max(0, atk - targetDef);

  // Check Haki: natural, from Armament buff, automatic from T7+, or water element
  const hasArmamentBuff = attacker.modifiers.some((m) => m.source === "armament_haki");
  const hasHaki =
    (def.naturalHaki && def.naturalHaki.length > 0) ||
    hasArmamentBuff ||
    state.turnNumber >= 7 ||
    attackElement === "water";

  const pending: PendingAttack = {
    attackerId: attackerInstanceId,
    targetId: targetInstanceId,
    targetIsCaptain,
    isSpecial: false,
    rawDamage,
    element: attackElement,
    attackTraits,
    hasHaki: hasHaki ?? false,
  };

  let next = produce(state, (draft) => {
    draft.cards[attackerInstanceId].tapped = true;
    draft.cards[attackerInstanceId].usedBaseAction = true;
    draft.pendingAttack = pending;
  });

  const targetName = targetIsCaptain
    ? "Capitaine"
    : getCardDef(state.cards[targetInstanceId].defId).name;
  next = addLog(
    next,
    attacker.owner,
    `${def.name} attaque ${targetName} (ATK ${atk} vs DEF ${targetDef} = ${rawDamage} degats)`
  );

  return next;
}

/**
 * Declare a special attack (costs Vol., does NOT tap — can combo with base).
 */
export function declareSpecialAttack(
  state: GameState,
  attackerInstanceId: string,
  targetInstanceId: string,
  targetIsCaptain: boolean
): GameState {
  const attacker = state.cards[attackerInstanceId];
  if (!attacker) throw new Error("Attacker not found");
  if (attacker.usedSpecialAttack) throw new Error("Special already used");
  if (hasSummoningSickness(state, attackerInstanceId)) {
    throw new Error("Character has summoning sickness");
  }

  const def = getCardDef(attacker.defId);

  // Trigger trap on attacker if present
  const trapEffectSpec = attacker.statusEffects.find((e) => e.type === "trap");
  if (trapEffectSpec) {
    state = produce(state, (draft) => {
      const a = draft.cards[attackerInstanceId];
      a.currentPv -= trapEffectSpec.damagePerTurn;
      a.statusEffects = a.statusEffects.filter((e) => e.type !== "trap");
    });
    state = addLog(
      state,
      attacker.owner,
      `Piege ! ${def.name} subit ${trapEffectSpec.damagePerTurn} degats en attaquant !`
    );
    if (state.cards[attackerInstanceId].currentPv <= 0) {
      state = addLog(state, attacker.owner, `${def.name} est KO par le piege !`);
      state = removeFromBoard(state, attackerInstanceId);
      return state;
    }
  }

  const spec = def.specialAttack;
  if (!spec) throw new Error("Character has no special attack");

  // Check 1x/game
  if (spec.oncePerGame && attacker.usedOnceAbilities.includes(spec.name)) {
    throw new Error("Already used this ability (1x/game)");
  }

  if (!canAfford(state, attacker.owner, spec.cost)) {
    throw new Error(`Cannot afford special (cost ${spec.cost})`);
  }

  let next = spendVolonte(state, attacker.owner, spec.cost);

  const baseAtk = getEffectiveAtk(state, attackerInstanceId);
  const totalAtk = baseAtk + spec.atkBonus;

  const attackTraits: AttackTrait[] = spec.attackTraits ?? [];

  let targetDefVal = 0;
  if (targetIsCaptain) {
    const opponent = getOpponent(attacker.owner);
    const cap = next.players[opponent].captain;
    const capDef = getCaptainDef(cap.defId);
    targetDefVal = cap.flipped ? capDef.verso.def : capDef.recto.def;
    for (const mod of cap.modifiers) {
      if (mod.stat === "def") targetDefVal += mod.amount;
    }
  } else {
    targetDefVal = getEffectiveDef(next, targetInstanceId);
  }

  // Piercing
  const isPiercing =
    attackTraits.includes("piercing") ||
    (def.traits?.includes("piercing") ?? false);
  if (isPiercing) {
    targetDefVal = Math.floor(targetDefVal / 2);
  }

  // Ignore DEF
  if (spec.ignoreDef) {
    targetDefVal = Math.max(0, targetDefVal - spec.ignoreDef);
  }

  const rawDamage = Math.max(0, totalAtk - targetDefVal);

  // Check Haki: natural, from Armament buff, or automatic from T7+
  const hasArmamentBuffSpec = attacker.modifiers.some((m) => m.source === "armament_haki");
  const hasHaki =
    (def.naturalHaki && def.naturalHaki.length > 0) ||
    hasArmamentBuffSpec ||
    state.turnNumber >= 7;

  const pending: PendingAttack = {
    attackerId: attackerInstanceId,
    targetId: targetInstanceId,
    targetIsCaptain,
    isSpecial: true,
    rawDamage,
    element: spec.element,
    attackTraits,
    hasHaki: hasHaki ?? false,
  };

  next = produce(next, (draft) => {
    draft.cards[attackerInstanceId].usedSpecialAttack = true;
    if (spec.oncePerGame) {
      draft.cards[attackerInstanceId].usedOnceAbilities.push(spec.name);
    }
    draft.pendingAttack = pending;
  });

  const targetName = targetIsCaptain
    ? "Capitaine"
    : getCardDef(next.cards[targetInstanceId].defId).name;
  next = addLog(
    next,
    attacker.owner,
    `${def.name} utilise ${spec.name} sur ${targetName} (ATK ${totalAtk} vs DEF ${targetDefVal} = ${rawDamage} degats)`
  );

  return next;
}

/**
 * Declare a fruit awakening special attack.
 */
export function declareFruitSpecialAttack(
  state: GameState,
  attackerInstanceId: string,
  fruitInstanceId: string,
  targetInstanceId: string,
  targetIsCaptain: boolean
): GameState {
  const attacker = state.cards[attackerInstanceId];
  if (!attacker) throw new Error("Attacker not found");
  if (hasSummoningSickness(state, attackerInstanceId)) {
    throw new Error("Character has summoning sickness");
  }

  const fruitCard = state.cards[fruitInstanceId];
  if (!fruitCard || !fruitCard.isAwakened) throw new Error("Fruit not awakened");

  const fruitDef = getCardDef(fruitCard.defId);
  const spec = fruitDef.fruitEffects?.awakening?.specialAttack;
  if (!spec) throw new Error("Fruit has no awakening special attack");

  // Check once per game
  if (spec.oncePerGame && attacker.usedOnceAbilities.includes(spec.name)) {
    throw new Error("Already used this fruit ability (1x/game)");
  }

  if (!canAfford(state, attacker.owner, spec.cost)) {
    throw new Error(`Cannot afford fruit special (cost ${spec.cost})`);
  }

  let next: GameState = spendVolonte(state, attacker.owner, spec.cost);

  const def = getCardDef(attacker.defId);
  const baseAtk = getEffectiveAtk(next, attackerInstanceId);
  const totalAtk = baseAtk + spec.atkBonus;

  let targetDefVal = 0;
  if (targetIsCaptain) {
    const opponent = getOpponent(attacker.owner);
    const cap = next.players[opponent].captain;
    const capDef = getCaptainDef(cap.defId);
    targetDefVal = cap.flipped ? capDef.verso.def : capDef.recto.def;
    for (const mod of cap.modifiers) {
      if (mod.stat === "def") targetDefVal += mod.amount;
    }
  } else {
    targetDefVal = getEffectiveDef(next, targetInstanceId);
  }

  const rawDamage = Math.max(0, totalAtk - targetDefVal);

  const hasArmament = attacker.modifiers.some((m) => m.source === "armament_haki");
  const hasHaki =
    (def.naturalHaki && def.naturalHaki.length > 0) || hasArmament || next.turnNumber >= 7;

  const pending: PendingAttack = {
    attackerId: attackerInstanceId,
    targetId: targetInstanceId,
    targetIsCaptain,
    isSpecial: true,
    rawDamage,
    element: undefined,
    attackTraits: [],
    hasHaki: hasHaki ?? false,
  };

  next = produce(next, (draft) => {
    if (spec.oncePerGame) {
      draft.cards[attackerInstanceId].usedOnceAbilities.push(spec.name);
    }
    draft.pendingAttack = pending;
  });

  const targetName = targetIsCaptain
    ? "Capitaine"
    : getCardDef(next.cards[targetInstanceId].defId).name;
  next = addLog(
    next,
    attacker.owner,
    `${def.name} utilise ${spec.name} sur ${targetName} (ATK ${totalAtk} vs DEF ${targetDefVal} = ${rawDamage} degats)`
  );

  return next;
}

// ============================================================
// Step 2: Counter Window — handled by UI/AI (pass or play counter)
// ============================================================

/**
 * Apply a counter card that reduces damage.
 */
export function applyCounterReduce(
  state: GameState,
  counterInstanceId: string
): GameState {
  if (!state.pendingAttack) throw new Error("No pending attack");

  const counter = state.cards[counterInstanceId];
  if (!counter) throw new Error("Counter not found");
  const counterDef = getCardDef(counter.defId);
  if (!counterDef.counterEffect) throw new Error("Not a counter card");
  if (counterDef.counterEffect.type !== "reduceDamage") {
    throw new Error("Not a damage reduction counter");
  }

  const owner = counter.owner;
  if (!canAfford(state, owner, counterDef.cost)) {
    throw new Error("Cannot afford counter");
  }

  let reduction = counterDef.counterEffect.amount;
  // Captain bonus: if targeting captain, extra reduction
  if (state.pendingAttack.targetIsCaptain && counterDef.counterEffect.captainBonus) {
    reduction = counterDef.counterEffect.captainBonus;
  }

  let next = spendVolonte(state, owner, counterDef.cost);

  next = produce(next, (draft) => {
    const p = draft.players[owner];
    // Remove counter from hand
    p.hand = p.hand.filter((id) => id !== counterInstanceId);
    // Send to graveyard
    draft.cards[counterInstanceId].zone = "graveyard";
    p.graveyard.push(counterInstanceId);
    // Reduce pending damage
    if (draft.pendingAttack) {
      draft.pendingAttack.rawDamage = Math.max(
        0,
        draft.pendingAttack.rawDamage - reduction
      );
    }
  });

  next = addLog(
    next,
    owner,
    `Joue ${counterDef.name} : reduit les degats de ${reduction}`
  );
  return next;
}

/**
 * Apply a "survive" counter (ally survives at 1 PV).
 * This is resolved at damage application time — mark the counter as played.
 */
export function applyCounterSurvive(
  state: GameState,
  counterInstanceId: string
): GameState {
  if (!state.pendingAttack) throw new Error("No pending attack");

  const counter = state.cards[counterInstanceId];
  if (!counter) throw new Error("Counter not found");
  const counterDef = getCardDef(counter.defId);
  if (counterDef.counterEffect?.type !== "survive") {
    throw new Error("Not a survive counter");
  }

  const owner = counter.owner;
  if (!canAfford(state, owner, counterDef.cost)) {
    throw new Error("Cannot afford counter");
  }

  let next = spendVolonte(state, owner, counterDef.cost);

  // We'll mark the pending attack with a "survive" flag by setting damage to a special state
  // Actually, we resolve this at applyDamage: if target would die and survive counter was played, set PV to 1
  // For now, just discard the counter and track it
  next = produce(next, (draft) => {
    const p = draft.players[owner];
    p.hand = p.hand.filter((id) => id !== counterInstanceId);
    draft.cards[counterInstanceId].zone = "graveyard";
    p.graveyard.push(counterInstanceId);
    // Mark the pending attack — target survives at 1 PV
    if (draft.pendingAttack) {
      (draft.pendingAttack as PendingAttack & { survivePlayed?: boolean }).survivePlayed = true;
    }
  });

  next = addLog(next, owner, `Joue ${counterDef.name} : survie a 1 PV !`);
  return next;
}

// ============================================================
// Step 3: Resolve Attack (apply damage)
// ============================================================

/**
 * Resolve the pending attack — apply damage to target.
 */
export function resolveAttack(state: GameState): GameState {
  const pending = state.pendingAttack as PendingAttack & { survivePlayed?: boolean } | null;
  if (!pending) throw new Error("No pending attack");

  let next = state;

  if (pending.targetIsCaptain) {
    next = applyCaptainDamage(next, pending);
  } else {
    next = applyCharacterDamage(next, pending);
  }

  // Apply element effects
  next = applyElementEffects(next, pending);

  // Check KO from element effects (thunder propagation, sand, water x2)
  if (!pending.targetIsCaptain) {
    const targetAfter = next.cards[pending.targetId];
    if (targetAfter && targetAfter.zone === "board" && targetAfter.currentPv <= 0) {
      const targetDefAfter = getCardDef(targetAfter.defId);
      const atkOwner = getAttackerOwner(next, pending.attackerId);
      next = addLog(next, targetAfter.owner, `${targetDefAfter.name} est KO (effet elementaire) !`);
      next = grantKOBonus(next, atkOwner);
      const koDefId = targetAfter.defId;
      const koOwner = targetAfter.owner;
      next = removeFromBoard(next, pending.targetId);
      const { applyOnKOEffects } = require("./passives");
      next = applyOnKOEffects(next, koOwner, atkOwner, koDefId);
    }
  }

  // Check KO from thunder propagation on adjacent characters
  if (pending.element === "thunder" && !pending.targetIsCaptain) {
    const target = state.cards[pending.targetId];
    if (target?.slot) {
      const adj = getAdjacentSlots(target.slot);
      const opponentId = getOpponent(getAttackerOwner(state, pending.attackerId));
      for (const adjSlot of adj) {
        const adjId = next.players[opponentId].board[adjSlot];
        if (adjId) {
          const adjCard = next.cards[adjId];
          if (adjCard && adjCard.zone === "board" && adjCard.currentPv <= 0) {
            const adjDef = getCardDef(adjCard.defId);
            const atkOwner = getAttackerOwner(next, pending.attackerId);
            next = addLog(next, adjCard.owner, `${adjDef.name} est KO (foudre) !`);
            next = grantKOBonus(next, atkOwner);
            next = removeFromBoard(next, adjId);
          }
          break;
        }
      }
    }
  }

  // Clear pending attack
  next = produce(next, (draft) => {
    draft.pendingAttack = null;
  });

  // Check win condition
  const winner = checkWinCondition(next);
  if (winner) {
    next = produce(next, (draft) => {
      draft.winner = winner;
    });
  }

  return next;
}

function getAttackerOwner(state: GameState, attackerId: string): PlayerId {
  // Captain attacker IDs look like "captain_player1" or "captain_player2"
  if (attackerId.startsWith("captain_")) {
    return attackerId.replace("captain_", "") as PlayerId;
  }
  const card = state.cards[attackerId];
  if (!card) throw new Error(`Attacker not found: ${attackerId}`);
  return card.owner;
}

function applyCaptainDamage(
  state: GameState,
  pending: PendingAttack & { survivePlayed?: boolean }
): GameState {
  const attackerOwner = getAttackerOwner(state, pending.attackerId);
  const opponentId = getOpponent(attackerOwner);
  const cap = state.players[opponentId].captain;

  // Logia check on captain
  if (cap.flipped) {
    const capDef = getCaptainDef(cap.defId);
    const isLogia = capDef.verso.traits?.includes("logia") ?? false;
    if (isLogia && !pending.hasHaki && pending.rawDamage > 0) {
      return addLog(
        state,
        attackerOwner,
        `⚠ ${capDef.name} : INTANGIBILITE LOGIA ! L'attaque passe a travers. Utilisez le Haki (T7+) ou l'Eau pour le toucher.`
      );
    }
  }

  let damage = pending.rawDamage;

  return produce(state, (draft) => {
    const targetCap = draft.players[opponentId].captain;
    targetCap.currentPv -= damage;
    if (pending.survivePlayed && targetCap.currentPv <= 0) {
      targetCap.currentPv = 1;
    }
    draft.log.push({
      turn: draft.turnNumber,
      player: attackerOwner,
      message: `Capitaine ${getCaptainDef(targetCap.defId).name} subit ${damage} degats (PV: ${targetCap.currentPv})`,
    });
  });
}

function applyCharacterDamage(
  state: GameState,
  pending: PendingAttack & { survivePlayed?: boolean }
): GameState {
  const target = state.cards[pending.targetId];
  if (!target) return state;

  const targetDef = getCardDef(target.defId);

  // Logia check (includes traits from equipped Devil Fruits)
  const isLogia = hasTrait(state, pending.targetId, "logia");
  if (isLogia && !pending.hasHaki && pending.rawDamage > 0) {
    // Check if logia already used this turn
    if (!target.logiaUsedThisTurn) {
      let next = produce(state, (draft) => {
        draft.cards[pending.targetId].logiaUsedThisTurn = true;
      });
      return addLog(
        next,
        getAttackerOwner(state, pending.attackerId),
        `⚠ ${targetDef.name} : INTANGIBILITE LOGIA ! Utilisez le Haki (T7+) ou l'Eau.`
      );
    }
  }

  let damage = pending.rawDamage;

  let next = produce(state, (draft) => {
    const t = draft.cards[pending.targetId];
    t.currentPv -= damage;
    if (pending.survivePlayed && t.currentPv <= 0) {
      t.currentPv = 1;
    }
  });

  const attackerOwner = getAttackerOwner(state, pending.attackerId);
  next = addLog(
    next,
    attackerOwner,
    `${targetDef.name} subit ${damage} degats (PV: ${next.cards[pending.targetId].currentPv})`
  );

  // Check KO
  if (next.cards[pending.targetId].currentPv <= 0) {
    next = addLog(next, target.owner, `${targetDef.name} est KO !`);
    next = grantKOBonus(next, attackerOwner);
    const koDefId = target.defId;
    const koOwner = target.owner;
    next = removeFromBoard(next, pending.targetId);
    // Apply on-KO effects (captain bonuses, synergy rage, recalculate buffs)
    const { applyOnKOEffects } = require("./passives");
    next = applyOnKOEffects(next, koOwner, attackerOwner, koDefId);
  }

  return next;
}

/**
 * Apply element effects after damage.
 */
function applyElementEffects(
  state: GameState,
  pending: PendingAttack
): GameState {
  if (!pending.element) return state;
  if (pending.targetIsCaptain) {
    // Apply element to captain
    return applyElementToCaptain(state, pending);
  }

  const target = state.cards[pending.targetId];
  if (!target || target.zone !== "board") return state;

  let next = state;

  switch (pending.element) {
    case "fire":
      // Burn: 1 dmg/turn for 2 turns
      next = produce(next, (draft) => {
        draft.cards[pending.targetId].statusEffects.push({
          type: "burn",
          turnsRemaining: 2,
          damagePerTurn: 1,
          source: pending.attackerId,
        });
      });
      break;

    case "ice":
      // Freeze: target loses next action
      // turnsRemaining: 2 so it survives the start-of-turn decrement and blocks for 1 full turn
      next = produce(next, (draft) => {
        draft.cards[pending.targetId].statusEffects.push({
          type: "freeze",
          turnsRemaining: 2,
          damagePerTurn: 0,
          source: pending.attackerId,
        });
      });
      break;

    case "thunder":
      // Propagate damage to 1 adjacent
      if (target.slot) {
        const adj = getAdjacentSlots(target.slot);
        const opponent = getOpponent(getAttackerOwner(state, pending.attackerId));
        for (const adjSlot of adj) {
          const adjId = state.players[opponent].board[adjSlot];
          if (adjId) {
            const propagateDmg = Math.max(1, Math.floor(pending.rawDamage / 2));
            next = produce(next, (draft) => {
              const adjCard = draft.cards[adjId];
              if (adjCard) {
                adjCard.currentPv -= propagateDmg;
              }
            });
            // Only propagate to 1
            break;
          }
        }
      }
      break;

    case "poison":
      // 1 dmg/turn permanent, can't kill
      next = produce(next, (draft) => {
        draft.cards[pending.targetId].statusEffects.push({
          type: "poison",
          turnsRemaining: -1, // permanent
          damagePerTurn: 1,
          source: pending.attackerId,
        });
      });
      break;

    case "sand":
      // -1 PV permanent (irrecoverable)
      next = produce(next, (draft) => {
        draft.cards[pending.targetId].currentPv -= 1;
      });
      break;

    case "water":
      // x2 damage vs Cursed — already factored into raw damage at declaration time?
      // For now, apply bonus damage if target is Cursed
      if (hasTrait(state, pending.targetId, "cursed")) {
        next = produce(next, (draft) => {
          const t = draft.cards[pending.targetId];
          if (t && t.zone === "board") {
            t.currentPv -= pending.rawDamage; // double = apply again
          }
        });
      }
      break;
  }

  return next;
}

function applyElementToCaptain(
  state: GameState,
  pending: PendingAttack
): GameState {
  const attackerOwner = getAttackerOwner(state, pending.attackerId);
  const opponentId = getOpponent(attackerOwner);

  let next = state;

  switch (pending.element) {
    case "fire":
      next = produce(next, (draft) => {
        draft.players[opponentId].captain.statusEffects.push({
          type: "burn",
          turnsRemaining: 2,
          damagePerTurn: 1,
          source: pending.attackerId,
        });
      });
      break;
    case "ice":
      next = produce(next, (draft) => {
        draft.players[opponentId].captain.statusEffects.push({
          type: "freeze",
          turnsRemaining: 1,
          damagePerTurn: 0,
          source: pending.attackerId,
        });
      });
      break;
    // Other elements on captain — simplified for MVP
  }

  return next;
}

// ============================================================
// Helpers for getting eligible counters
// ============================================================

/**
 * Get counter cards in hand that can be played during the counter window.
 */
export function getEligibleCounters(
  state: GameState,
  playerId: PlayerId
): string[] {
  if (!state.pendingAttack) return [];

  const player = state.players[playerId];
  return player.hand.filter((id) => {
    const card = state.cards[id];
    const def = getCardDef(card.defId);
    return def.type === "counter" && canAfford(state, playerId, def.cost);
  });
}

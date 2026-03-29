import type { GameState, GameAction, PlayerId, Slot } from "@/types";
import { getCardDef, getCaptainDef } from "./cardRegistry";
import {
  startTurn,
  endTurn,
  checkWinCondition,
  addLog,
  drawCard,
  getOpponent,
} from "./gameState";
import { canAfford, spendVolonte, gainVolonte } from "./volonte";
import {
  deployCharacter,
  equipObject,
  deployShip,
  moveCharacter,
  getEmptySlots,
  getBoardCharacters,
  getValidTargets,
  hasSummoningSickness,
  hasTrait,
  getEffectiveAtk,
  getEffectiveDef,
} from "./board";
import {
  declareBaseAttack,
  declareSpecialAttack,
  declareFruitSpecialAttack,
  resolveAttack,
  applyCounterReduce,
  applyCounterSurvive,
  getEligibleCounters,
} from "./combat";
import { canFlipCaptain, flipCaptain, declareCaptainBaseAttack } from "./captain";
import { isHakiAvailable, useObservationHaki, useArmamentHaki } from "./haki";
import { produce } from "immer";

// ============================================================
// Execute a game action — single entry point for all mutations
// ============================================================

export function executeAction(
  state: GameState,
  action: GameAction
): GameState {
  // Don't allow actions if game is over
  if (state.winner) return state;

  switch (action.type) {
    case "deployCharacter":
      return deployCharacter(state, state.currentPlayer, action.instanceId, action.slot);

    case "equipObject":
      return equipObject(state, state.currentPlayer, action.objectInstanceId, action.targetInstanceId);

    case "deployShip":
      return deployShip(state, state.currentPlayer, action.instanceId);

    case "baseAttack":
      return declareBaseAttack(
        state,
        action.attackerInstanceId,
        action.targetInstanceId,
        action.targetIsCaptain ?? false
      );

    case "specialAttack":
      return declareSpecialAttack(
        state,
        action.attackerInstanceId,
        action.targetInstanceId,
        action.targetIsCaptain ?? false
      );

    case "baseSupportAction":
      return executeSupportAction(state, state.currentPlayer, action.instanceId, action.targetInstanceId);

    case "playEvent":
      return playEvent(state, state.currentPlayer, action.instanceId);

    case "playCounter":
      return playCounter(state, action.instanceId);

    case "passCounter":
      // Resolve the pending attack without counter
      return resolveAttack(state);

    case "flipCaptain":
      return flipCaptain(state, state.currentPlayer, action.slot);

    case "captainAttack":
      return declareCaptainBaseAttack(
        state,
        state.currentPlayer,
        action.targetInstanceId,
        action.targetIsCaptain ?? false
      );

    case "activateShip":
      return activateShipAbility(state, state.currentPlayer, action.shipInstanceId);

    case "useHaki":
      return handleHaki(state, state.currentPlayer, action);

    case "moveCharacter":
      return moveCharacter(state, state.currentPlayer, action.instanceId, action.targetSlot);

    case "awakenFruit": {
      const { awakenFruit } = require("./fruits");
      return awakenFruit(state, state.currentPlayer, action.fruitInstanceId);
    }

    case "fruitSpecialAttack":
      return declareFruitSpecialAttack(
        state,
        action.attackerInstanceId,
        action.fruitInstanceId,
        action.targetInstanceId,
        action.targetIsCaptain ?? false
      );

    case "endTurn": {
      const next = endTurn(state);
      return startTurn(next);
    }

    default:
      return state;
  }
}

// ============================================================
// Play an event card
// ============================================================

function playEvent(
  state: GameState,
  playerId: PlayerId,
  instanceId: string
): GameState {
  const card = state.cards[instanceId];
  if (!card) throw new Error("Card not found");
  if (card.owner !== playerId) throw new Error("Not your card");
  if (card.zone !== "hand") throw new Error("Card not in hand");

  const def = getCardDef(card.defId);
  if (def.type !== "event") throw new Error("Not an event card");

  if (!canAfford(state, playerId, def.cost)) {
    throw new Error(`Cannot afford ${def.name}`);
  }

  let next = spendVolonte(state, playerId, def.cost);

  // Resolve event effect
  const effect = def.eventEffect;
  if (effect) {
    next = resolveEventEffect(next, playerId, effect, def.name);
  }

  // Discard the event
  next = produce(next, (draft) => {
    const p = draft.players[playerId];
    p.hand = p.hand.filter((id) => id !== instanceId);
    draft.cards[instanceId].zone = "graveyard";
    p.graveyard.push(instanceId);
  });

  next = addLog(next, playerId, `Joue ${def.name}`);
  return next;
}

function resolveEventEffect(
  state: GameState,
  playerId: PlayerId,
  effect: NonNullable<import("@/types").CardDef["eventEffect"]>,
  cardName: string
): GameState {
  let next = state;

  switch (effect.type) {
    case "gainWill":
      next = produce(next, (draft) => {
        draft.players[playerId].volonte += effect.amount;
      });
      break;

    case "draw": {
      for (let i = 0; i < effect.amount; i++) {
        next = drawCard(next, playerId);
      }
      if (effect.discard && effect.discard > 0) {
        // Auto-discard the oldest card(s) in hand (first in hand array, not the newly drawn ones)
        next = produce(next, (draft) => {
          const p = draft.players[playerId];
          for (let i = 0; i < (effect.discard ?? 0) && p.hand.length > 0; i++) {
            const discarded = p.hand.shift()!; // discard oldest, not newest
            draft.cards[discarded].zone = "graveyard";
            p.graveyard.push(discarded);
          }
        });
        const discardCount = Math.min(effect.discard, next.players[playerId].hand.length + effect.discard);
        next = addLog(next, playerId, `Defausse automatique de ${effect.discard} carte(s) (plus ancienne en main).`);
      }
      break;
    }

    case "healAlly": {
      if (effect.allAllies) {
        next = produce(next, (draft) => {
          const p = draft.players[playerId];
          for (const slot of Object.values(p.board)) {
            if (slot) {
              const card = draft.cards[slot];
              if (card) {
                const def = getCardDef(card.defId);
                card.currentPv = Math.min(
                  card.currentPv + effect.amount,
                  def.pv ?? card.currentPv + effect.amount
                );
              }
            }
          }
        });
      }
      break;
    }

    case "buffAllies": {
      next = produce(next, (draft) => {
        const p = draft.players[playerId];
        for (const slot of Object.values(p.board)) {
          if (slot) {
            const card = draft.cards[slot];
            if (card) {
              card.modifiers.push({
                id: `event_${cardName}_${Date.now()}`,
                stat: effect.stat,
                amount: effect.amount,
                source: cardName,
                duration: effect.duration === "turn" ? "turn" : "permanent",
              });
            }
          }
        }
      });
      break;
    }

    case "damageEnemies": {
      const opponentId = getOpponent(playerId);
      if (effect.target === "allFront") {
        next = produce(next, (draft) => {
          const opp = draft.players[opponentId];
          for (const slotKey of ["V1", "V2", "V3"] as const) {
            const id = opp.board[slotKey];
            if (id) {
              draft.cards[id].currentPv -= effect.amount;
            }
          }
        });
      } else if (effect.target === "allCursed") {
        next = produce(next, (draft) => {
          const opp = draft.players[opponentId];
          for (const slot of Object.values(opp.board)) {
            if (slot) {
              const card = draft.cards[slot];
              if (card) {
                const def = getCardDef(card.defId);
                if (def.traits?.includes("cursed")) {
                  card.currentPv -= effect.amount;
                }
              }
            }
          }
        });
      }
      break;
    }

    case "dodgeAll":
      // All characters dodge all attacks this turn — simplified as a flag
      // For MVP, this is a no-op (would need combat phase tracking)
      break;

    case "custom":
      // Custom effects handled per card ID
      break;
  }

  return next;
}

// ============================================================
// Play a counter card
// ============================================================

function playCounter(state: GameState, instanceId: string): GameState {
  const card = state.cards[instanceId];
  if (!card) throw new Error("Counter not found");

  const def = getCardDef(card.defId);
  if (def.type !== "counter") throw new Error("Not a counter card");

  const counterEffect = def.counterEffect;
  if (!counterEffect) throw new Error("No counter effect");

  switch (counterEffect.type) {
    case "reduceDamage":
      return applyCounterReduce(state, instanceId);
    case "survive":
      return applyCounterSurvive(state, instanceId);
    default:
      return state;
  }
}

// ============================================================
// Ship ability activation
// ============================================================

function activateShipAbility(
  state: GameState,
  playerId: PlayerId,
  shipInstanceId: string
): GameState {
  const ship = state.cards[shipInstanceId];
  if (!ship) throw new Error("Ship not found");

  const def = getCardDef(ship.defId);
  if (!def.shipActive) throw new Error("Ship has no active ability");

  const active = def.shipActive;

  if (active.oncePerGame && ship.usedOnceAbilities.includes(active.name)) {
    throw new Error("Ship ability already used (1x/game)");
  }

  if (!canAfford(state, playerId, active.cost)) {
    throw new Error(`Cannot afford ${active.name} (cost ${active.cost})`);
  }

  let next = active.cost > 0 ? spendVolonte(state, playerId, active.cost) : state;

  if (active.oncePerGame) {
    next = produce(next, (draft) => {
      draft.cards[shipInstanceId].usedOnceAbilities.push(active.name);
    });
  }

  const desc = active.description.toLowerCase();

  // Damage to front line ("deg. a toute la Ligne Avant ennemie")
  if (desc.includes("deg.") && desc.includes("avant")) {
    const dmgMatch = active.description.match(/(\d+)\s*deg/);
    const dmg = dmgMatch ? parseInt(dmgMatch[1]) : 0;
    if (dmg > 0) {
      const opponentId = getOpponent(playerId);
      next = produce(next, (draft) => {
        const opp = draft.players[opponentId];
        for (const slotKey of ["V1", "V2", "V3"] as const) {
          const id = opp.board[slotKey];
          if (id) {
            draft.cards[id].currentPv -= dmg;
          }
        }
      });
    }
    next = addLog(next, playerId, `${def.name} active ${active.name} !`);
    return next;
  }

  // Buff all allies ("tous allies/Marines +X ATK +Y DEF")
  if (desc.includes("+") && (desc.includes("atk") || desc.includes("def"))) {
    const atkMatch = desc.match(/\+(\d+)\s*atk/i);
    const defMatch = desc.match(/\+(\d+)\s*def/i);
    const atkBuff = atkMatch ? parseInt(atkMatch[1]) : 0;
    const defBuff = defMatch ? parseInt(defMatch[1]) : 0;

    next = produce(next, (draft) => {
      const p = draft.players[playerId];
      for (const slot of Object.values(p.board)) {
        if (slot) {
          const c = draft.cards[slot];
          if (c) {
            if (atkBuff > 0) {
              c.modifiers.push({
                id: `ship_${active.name}_atk_${Date.now()}`,
                stat: "atk",
                amount: atkBuff,
                source: `ship_${def.id}`,
                duration: "turn",
              });
            }
            if (defBuff > 0) {
              c.modifiers.push({
                id: `ship_${active.name}_def_${Date.now()}`,
                stat: "def",
                amount: defBuff,
                source: `ship_${def.id}`,
                duration: "turn",
              });
            }
          }
        }
      }
    });
    next = addLog(next, playerId, `${def.name} active ${active.name} !`);
    return next;
  }

  // Fallback: just log the activation
  next = addLog(next, playerId, `${def.name} active ${active.name} !`);
  return next;
}

// ============================================================
// Support actions (Usopp trap, Robin immobilize, Chopper heal, etc.)
// ============================================================

function executeSupportAction(
  state: GameState,
  playerId: PlayerId,
  instanceId: string,
  targetInstanceId?: string
): GameState {
  const card = state.cards[instanceId];
  if (!card) throw new Error("Card not found");
  if (card.owner !== playerId) throw new Error("Not your card");
  if (card.tapped || card.usedBaseAction) throw new Error("Action already used");

  const def = getCardDef(card.defId);
  const ba = def.baseAction;
  if (!ba?.isSupport) throw new Error("Not a support action");

  let next = produce(state, (draft) => {
    draft.cards[instanceId].tapped = true;
    draft.cards[instanceId].usedBaseAction = true;
  });

  // Immobilize (Robin, Kuzan)
  if (ba.immobilize && targetInstanceId) {
    next = produce(next, (draft) => {
      const target = draft.cards[targetInstanceId];
      if (target) {
        target.statusEffects.push({
          type: "immobilize",
          turnsRemaining: 2, // survives start-of-turn decrement, blocks for 1 turn
          damagePerTurn: 0,
          source: instanceId,
        });
      }
    });
    const targetName = getCardDef(state.cards[targetInstanceId!].defId).name;
    next = addLog(next, playerId, `${def.name} utilise ${ba.name} : ${targetName} est immobilise !`);
    return next;
  }

  // Trap (Usopp)
  if (ba.description?.includes("piege") || ba.description?.includes("Piege")) {
    if (!targetInstanceId) throw new Error("Trap needs a target");
    next = produce(next, (draft) => {
      const target = draft.cards[targetInstanceId];
      if (target) {
        target.statusEffects.push({
          type: "trap",
          turnsRemaining: -1, // permanent until triggered
          damagePerTurn: 3,
          source: instanceId,
        });
      }
    });
    const targetName = getCardDef(state.cards[targetInstanceId].defId).name;
    next = addLog(next, playerId, `${def.name} utilise ${ba.name} : piege pose sur ${targetName} !`);
    return next;
  }

  // Heal (Chopper, Marine soldier)
  if (ba.healAmount && targetInstanceId) {
    const targetCard = state.cards[targetInstanceId];
    const targetDef = getCardDef(targetCard.defId);
    next = produce(next, (draft) => {
      const target = draft.cards[targetInstanceId];
      if (target) {
        target.currentPv = Math.min(
          target.currentPv + ba.healAmount!,
          targetDef.pv ?? target.currentPv + ba.healAmount!
        );
      }
    });
    next = addLog(next, playerId, `${def.name} utilise ${ba.name} : +${ba.healAmount} PV a ${targetDef.name}`);
    return next;
  }

  // Global buff (Brook "New World", Sengoku "Commandement", Nami "Mirage Tempo")
  // Apply a generic +1 ATK buff to all allies for the turn as default
  next = produce(next, (draft) => {
    const p = draft.players[playerId];
    for (const slot of Object.values(p.board)) {
      if (slot) {
        const c = draft.cards[slot];
        if (c) {
          c.modifiers.push({
            id: `support_${ba.name}_${Date.now()}`,
            stat: "atk",
            amount: 1,
            source: `support_${def.id}`,
            duration: "turn",
          });
        }
      }
    }
  });
  next = addLog(next, playerId, `${def.name} utilise ${ba.name} !`);

  return next;
}

// ============================================================
// Haki handler
// ============================================================

function handleHaki(
  state: GameState,
  playerId: PlayerId,
  action: Extract<GameAction, { type: "useHaki" }>
): GameState {
  switch (action.hakiType) {
    case "observation":
      return useObservationHaki(state, playerId);
    case "armament":
      if (!action.targetInstanceId) throw new Error("Armament needs a target");
      return useArmamentHaki(state, playerId, action.targetInstanceId);
    default:
      return state;
  }
}

// ============================================================
// Get all valid actions for the current player
// ============================================================

export function getValidActions(
  state: GameState,
  playerId: PlayerId
): GameAction[] {
  const actions: GameAction[] = [];
  const player = state.players[playerId];

  // If there's a pending attack, only counter actions are valid
  if (state.pendingAttack) {
    const defenderId = getOpponent(state.currentPlayer);
    if (playerId === defenderId) {
      const counters = getEligibleCounters(state, playerId);
      for (const id of counters) {
        actions.push({ type: "playCounter", instanceId: id });
      }
      // Can always pass
      actions.push({ type: "passCounter" });

      // Observation Haki to dodge
      if (isHakiAvailable(state, playerId, "observation")) {
        actions.push({ type: "useHaki", hakiType: "observation" });
      }
    }
    return actions;
  }

  // Only current player can act during main phase
  if (playerId !== state.currentPlayer) return actions;
  if (state.phase !== "main") return actions;

  // Deploy characters from hand
  const emptySlots = getEmptySlots(state, playerId);
  for (const cardId of player.hand) {
    const card = state.cards[cardId];
    const def = getCardDef(card.defId);
    if (def.type === "character" && canAfford(state, playerId, def.cost)) {
      for (const slot of emptySlots) {
        actions.push({ type: "deployCharacter", instanceId: cardId, slot });
      }
    }
  }

  // Deploy ships from hand
  for (const cardId of player.hand) {
    const card = state.cards[cardId];
    const def = getCardDef(card.defId);
    if (def.type === "ship" && canAfford(state, playerId, def.cost)) {
      actions.push({ type: "deployShip", instanceId: cardId });
    }
  }

  // Equip objects
  const boardChars = getBoardCharacters(state, playerId);
  for (const cardId of player.hand) {
    const card = state.cards[cardId];
    const def = getCardDef(card.defId);
    if (def.type === "object" && canAfford(state, playerId, def.cost)) {
      for (const target of boardChars) {
        actions.push({
          type: "equipObject",
          objectInstanceId: cardId,
          targetInstanceId: target.instanceId,
        });
      }
    }
  }

  // Play events
  for (const cardId of player.hand) {
    const card = state.cards[cardId];
    const def = getCardDef(card.defId);
    if (def.type === "event" && canAfford(state, playerId, def.cost)) {
      actions.push({ type: "playEvent", instanceId: cardId });
    }
  }

  // Support base actions (Usopp trap, Robin immobilize, Chopper heal, Brook buff, etc.)
  for (const char of boardChars) {
    if (char.tapped || char.usedBaseAction) continue;
    if (hasSummoningSickness(state, char.instanceId)) continue;
    if (char.statusEffects.some((e) => e.type === "freeze" || e.type === "immobilize")) continue;

    const def = getCardDef(char.defId);
    if (!def.baseAction?.isSupport) continue;

    const ba = def.baseAction;

    if (ba.immobilize) {
      // Target: any enemy character
      const oppChars = getBoardCharacters(state, getOpponent(playerId));
      for (const opp of oppChars) {
        actions.push({ type: "baseSupportAction", instanceId: char.instanceId, targetInstanceId: opp.instanceId });
      }
    } else if (ba.description?.includes("piege") || ba.description?.includes("Piege")) {
      // Trap: target any enemy character
      const oppChars = getBoardCharacters(state, getOpponent(playerId));
      for (const opp of oppChars) {
        actions.push({ type: "baseSupportAction", instanceId: char.instanceId, targetInstanceId: opp.instanceId });
      }
    } else if (ba.healAmount) {
      // Heal: target any friendly character on board
      for (const ally of boardChars) {
        if (ally.instanceId === char.instanceId) continue;
        actions.push({ type: "baseSupportAction", instanceId: char.instanceId, targetInstanceId: ally.instanceId });
      }
    } else {
      // Global buff (no specific target needed)
      actions.push({ type: "baseSupportAction", instanceId: char.instanceId });
    }
  }

  // Base attacks — ALL characters with ATK > 0 can base attack
  // (even support chars like Chopper ATK 1 — they do their effect + attack)
  for (const char of boardChars) {
    if (char.tapped || char.usedBaseAction) continue;
    if (hasSummoningSickness(state, char.instanceId)) continue;

    const def = getCardDef(char.defId);
    // Must have effective ATK > 0 to attack (includes equipment + modifiers)
    if (getEffectiveAtk(state, char.instanceId) <= 0) continue;

    if (char.statusEffects.some((e) => e.type === "freeze" || e.type === "immobilize")) continue;

    // Check cannotAttackFemale passive
    const cannotAttackFemale = def.passive?.effects.some(
      (e) => e.type === "cannotAttackFemale"
    ) ?? false;

    const targets = getValidTargets(state, char.instanceId, false);
    for (const targetId of targets.characterTargets) {
      // Filter female targets if cannotAttackFemale
      if (cannotAttackFemale) {
        const targetCard = state.cards[targetId];
        if (targetCard) {
          const targetDef = getCardDef(targetCard.defId);
          if (targetDef.tags?.includes("female")) continue;
        }
      }
      actions.push({
        type: "baseAttack",
        attackerInstanceId: char.instanceId,
        targetInstanceId: targetId,
      });
    }
    if (targets.canTargetCaptain) {
      actions.push({
        type: "baseAttack",
        attackerInstanceId: char.instanceId,
        targetInstanceId: `captain_${getOpponent(playerId)}`,
        targetIsCaptain: true,
      });
    }
  }

  // Special attacks (all characters with specials, including support chars)
  for (const char of boardChars) {
    if (char.usedSpecialAttack) continue;
    if (hasSummoningSickness(state, char.instanceId)) continue;

    const def = getCardDef(char.defId);
    if (!def.specialAttack) continue;
    // Support specials (heal, buff) don't need attack targets — skip for now
    if (def.specialAttack.isSupport) continue;
    if (def.specialAttack.oncePerGame && char.usedOnceAbilities.includes(def.specialAttack.name)) continue;
    if (!canAfford(state, playerId, def.specialAttack.cost)) continue;

    if (char.statusEffects.some((e) => e.type === "freeze" || e.type === "immobilize")) continue;

    // Check cannotAttackFemale
    const cantFemale = def.passive?.effects.some(
      (e) => e.type === "cannotAttackFemale"
    ) ?? false;

    const targets = getValidTargets(state, char.instanceId, true);
    for (const targetId of targets.characterTargets) {
      if (cantFemale) {
        const tc = state.cards[targetId];
        if (tc && getCardDef(tc.defId).tags?.includes("female")) continue;
      }
      actions.push({
        type: "specialAttack",
        attackerInstanceId: char.instanceId,
        targetInstanceId: targetId,
      });
    }
    if (targets.canTargetCaptain) {
      actions.push({
        type: "specialAttack",
        attackerInstanceId: char.instanceId,
        targetInstanceId: `captain_${getOpponent(playerId)}`,
        targetIsCaptain: true,
      });
    }
  }

  // Fruit awakening special attacks
  for (const char of boardChars) {
    if (hasSummoningSickness(state, char.instanceId)) continue;
    if (char.statusEffects.some((e) => e.type === "freeze" || e.type === "immobilize")) continue;

    for (const objId of char.attachedObjects) {
      const objCard = state.cards[objId];
      if (!objCard || !objCard.isAwakened) continue;
      const objDef = getCardDef(objCard.defId);
      const fruitSpec = objDef.fruitEffects?.awakening?.specialAttack;
      if (!fruitSpec) continue;
      if (fruitSpec.oncePerGame && char.usedOnceAbilities.includes(fruitSpec.name)) continue;
      if (!canAfford(state, playerId, fruitSpec.cost)) continue;

      const targets = getValidTargets(state, char.instanceId, true);
      for (const targetId of targets.characterTargets) {
        actions.push({
          type: "fruitSpecialAttack",
          attackerInstanceId: char.instanceId,
          fruitInstanceId: objId,
          targetInstanceId: targetId,
        });
      }
      if (targets.canTargetCaptain) {
        actions.push({
          type: "fruitSpecialAttack",
          attackerInstanceId: char.instanceId,
          fruitInstanceId: objId,
          targetInstanceId: `captain_${getOpponent(playerId)}`,
          targetIsCaptain: true,
        });
      }
    }
  }

  // Captain flip
  if (canFlipCaptain(state, playerId)) {
    for (const slot of emptySlots) {
      actions.push({ type: "flipCaptain", slot });
    }
  }

  // Captain attacks (if verso and on board)
  if (player.captain.flipped && player.captain.slot && !player.captain.tapped) {
    const capDef = getCaptainDef(player.captain.defId);
    if (player.captain.deployedTurn !== state.turnNumber || capDef.verso.traits?.includes("rush")) {
      // Can attack — simplified: target any enemy front or captain
      actions.push({
        type: "captainAttack",
        targetInstanceId: `captain_${getOpponent(playerId)}`,
        targetIsCaptain: true,
      });
      const oppChars = getBoardCharacters(state, getOpponent(playerId));
      for (const opp of oppChars) {
        actions.push({
          type: "captainAttack",
          targetInstanceId: opp.instanceId,
        });
      }
    }
  }

  // Free move
  if (!player.usedFreeMove) {
    for (const char of boardChars) {
      if (char.slot) {
        const { ADJACENCY } = require("./utils");
        const adjacent = ADJACENCY[char.slot] ?? [];
        for (const adjSlot of adjacent) {
          if (player.board[adjSlot as keyof typeof player.board] === null) {
            actions.push({
              type: "moveCharacter",
              instanceId: char.instanceId,
              targetSlot: adjSlot as import("@/types").Slot,
            });
          }
        }
      }
    }
  }

  // Activate ship ability
  if (player.activeShip) {
    const shipCard = state.cards[player.activeShip];
    if (shipCard) {
      const shipDef = getCardDef(shipCard.defId);
      if (shipDef.shipActive) {
        const sa = shipDef.shipActive;
        const canUse =
          (!sa.oncePerGame || !shipCard.usedOnceAbilities.includes(sa.name)) &&
          canAfford(state, playerId, sa.cost);
        if (canUse) {
          actions.push({ type: "activateShip", shipInstanceId: player.activeShip });
        }
      }
    }
  }

  // Awaken Devil Fruits
  const { canAwakenFruit } = require("./fruits");
  for (const char of boardChars) {
    for (const objId of char.attachedObjects) {
      const objCard = state.cards[objId];
      if (objCard) {
        const objDef = getCardDef(objCard.defId);
        if (objDef.subtype === "fruit" && canAwakenFruit(state, playerId, objId)) {
          actions.push({ type: "awakenFruit", fruitInstanceId: objId });
        }
      }
    }
  }

  // Armament Haki
  if (isHakiAvailable(state, playerId, "armament")) {
    for (const char of boardChars) {
      actions.push({
        type: "useHaki",
        hakiType: "armament",
        targetInstanceId: char.instanceId,
      });
    }
  }

  // End turn is always valid
  actions.push({ type: "endTurn" });

  return actions;
}

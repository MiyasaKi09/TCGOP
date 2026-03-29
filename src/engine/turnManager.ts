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

    case "useHaki":
      return handleHaki(state, state.currentPlayer, action);

    case "moveCharacter":
      return moveCharacter(state, state.currentPlayer, action.instanceId, action.targetSlot);

    case "endTurn": {
      const next = endTurn(state);
      // Start the next player's turn
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
        // For simplicity, auto-discard the last drawn cards
        // In a full implementation, the player would choose
        next = produce(next, (draft) => {
          const p = draft.players[playerId];
          for (let i = 0; i < (effect.discard ?? 0) && p.hand.length > 0; i++) {
            const discarded = p.hand.pop()!;
            draft.cards[discarded].zone = "graveyard";
            p.graveyard.push(discarded);
          }
        });
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

  // Base attacks — ALL characters with ATK > 0 can base attack
  // (even support chars like Chopper ATK 1 — they do their effect + attack)
  for (const char of boardChars) {
    if (char.tapped || char.usedBaseAction) continue;
    if (hasSummoningSickness(state, char.instanceId)) continue;

    const def = getCardDef(char.defId);
    // Must have ATK > 0 to attack
    if (!def.atk || def.atk <= 0) continue;

    if (char.statusEffects.some((e) => e.type === "freeze")) continue;

    const targets = getValidTargets(state, char.instanceId, false);
    for (const targetId of targets.characterTargets) {
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

    if (char.statusEffects.some((e) => e.type === "freeze")) continue;

    const targets = getValidTargets(state, char.instanceId, true);
    for (const targetId of targets.characterTargets) {
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

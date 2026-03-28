import { produce } from "immer";
import type { GameState, PlayerId, Slot, EntryEffect } from "@/types";
import { getCaptainDef } from "./cardRegistry";
import { canAfford, spendVolonte } from "./volonte";
import { addLog, getOpponent } from "./gameState";
import { getBoardCharacters, getEffectiveAtk } from "./board";

/**
 * Check if a player can flip their captain.
 */
export function canFlipCaptain(
  state: GameState,
  playerId: PlayerId
): boolean {
  const captain = state.players[playerId].captain;
  if (captain.flipped) return false; // Already flipped (irreversible)

  const def = getCaptainDef(captain.defId);
  const condition = def.flipCondition;

  // Check auto-flip condition (allies <= N)
  if (condition.autoIfAlliesLte !== undefined) {
    const allyCount = getBoardCharacters(state, playerId).length;
    if (allyCount <= condition.autoIfAlliesLte) return true;
  }

  // Check Vol. cost
  if (condition.cost !== undefined) {
    return canAfford(state, playerId, condition.cost);
  }

  return false;
}

/**
 * Flip the captain from recto to verso.
 * Places them in the specified slot on the board.
 * Triggers entry effect.
 */
export function flipCaptain(
  state: GameState,
  playerId: PlayerId,
  slot: Slot
): GameState {
  const captain = state.players[playerId].captain;
  if (captain.flipped) throw new Error("Captain already flipped");

  const def = getCaptainDef(captain.defId);
  const condition = def.flipCondition;

  // Check slot availability
  if (state.players[playerId].board[slot] !== null) {
    throw new Error(`Slot ${slot} is occupied`);
  }

  // Determine cost
  let cost = 0;
  const allyCount = getBoardCharacters(state, playerId).length;
  const autoFlip =
    condition.autoIfAlliesLte !== undefined &&
    allyCount <= condition.autoIfAlliesLte;

  if (!autoFlip) {
    cost = condition.cost ?? 0;
    if (!canAfford(state, playerId, cost)) {
      throw new Error("Cannot afford captain flip");
    }
  }

  let next = cost > 0 ? spendVolonte(state, playerId, cost) : state;

  // Flip captain
  next = produce(next, (draft) => {
    const cap = draft.players[playerId].captain;
    cap.flipped = true;
    cap.currentPv = def.verso.pv;
    cap.slot = slot;
    cap.deployedTurn = draft.turnNumber;
  });

  next = addLog(
    next,
    playerId,
    `${def.name} s'engage sur le champ de bataille ! (verso, slot ${slot})`
  );

  // Apply entry effect
  next = resolveEntryEffect(next, playerId, def.verso.entryEffect);

  return next;
}

/**
 * Resolve a captain's entry effect.
 */
function resolveEntryEffect(
  state: GameState,
  playerId: PlayerId,
  effect: EntryEffect
): GameState {
  let next = state;

  switch (effect.type) {
    case "buffAllies": {
      next = produce(next, (draft) => {
        const player = draft.players[playerId];
        for (const slot of Object.values(player.board)) {
          if (slot) {
            const card = draft.cards[slot];
            if (card) {
              card.modifiers.push({
                id: `entry_${Date.now()}`,
                stat: effect.stat,
                amount: effect.amount,
                source: player.captain.defId,
                duration: "turn",
              });
            }
          }
        }
      });
      next = addLog(
        next,
        playerId,
        `Effet d'entree : tous allies +${effect.amount} ${effect.stat.toUpperCase()} ce tour !`
      );
      break;
    }

    case "draw": {
      const { drawCard } = require("./gameState");
      for (let i = 0; i < effect.amount; i++) {
        next = drawCard(next, playerId);
      }
      next = addLog(next, playerId, `Effet d'entree : pioche ${effect.amount} carte(s)`);
      break;
    }

    case "damageEnemies": {
      const opponentId = getOpponent(playerId);
      const opponent = next.players[opponentId];

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
        next = addLog(
          next,
          playerId,
          `Effet d'entree : ${effect.amount} degats a toute la Ligne Avant ennemie !`
        );
      }
      break;
    }

    case "multi": {
      for (const sub of effect.effects) {
        next = resolveEntryEffect(next, playerId, sub);
      }
      break;
    }

    case "custom": {
      next = addLog(next, playerId, `Effet d'entree special : ${effect.description}`);
      break;
    }
  }

  return next;
}

/**
 * Declare a captain attack (costs Vol., captain has no free base action from recto).
 * Verso captain has a base action.
 */
export function declareCaptainBaseAttack(
  state: GameState,
  playerId: PlayerId,
  targetInstanceId: string,
  targetIsCaptain: boolean
): GameState {
  const captain = state.players[playerId].captain;
  if (!captain.flipped) throw new Error("Captain not flipped (verso required)");
  if (captain.tapped) throw new Error("Captain is tapped");
  if (captain.usedBaseAction) throw new Error("Captain base action already used");

  const def = getCaptainDef(captain.defId);
  const baseAction = def.verso.baseAction;

  // Captain summoning sickness
  if (captain.deployedTurn === state.turnNumber) {
    // Verso captain just flipped — has mal de terre unless Rush
    const hasRush = def.verso.traits?.includes("rush") ?? false;
    if (!hasRush) throw new Error("Captain has summoning sickness");
  }

  // Calculate ATK
  let atk = def.verso.atk;
  for (const mod of captain.modifiers) {
    if (mod.stat === "atk") atk += mod.amount;
  }

  // Get target DEF
  let targetDefVal = 0;
  if (targetIsCaptain) {
    const opponentId = getOpponent(playerId);
    const oppCap = state.players[opponentId].captain;
    const oppCapDef = getCaptainDef(oppCap.defId);
    targetDefVal = oppCap.flipped ? oppCapDef.verso.def : oppCapDef.recto.def;
    for (const mod of oppCap.modifiers) {
      if (mod.stat === "def") targetDefVal += mod.amount;
    }
  } else {
    targetDefVal = getEffectiveAtk(state, targetInstanceId); // oops — should be DEF
    // Fix: use getEffectiveDef
    const { getEffectiveDef } = require("./board");
    targetDefVal = getEffectiveDef(state, targetInstanceId);
  }

  const rawDamage = Math.max(0, atk - targetDefVal);

  const hasHaki =
    (def.verso.naturalHaki && def.verso.naturalHaki.length > 0) ?? false;

  let next = produce(state, (draft) => {
    const cap = draft.players[playerId].captain;
    cap.tapped = true;
    cap.usedBaseAction = true;
    draft.pendingAttack = {
      attackerId: `captain_${playerId}`,
      targetId: targetInstanceId,
      targetIsCaptain,
      isSpecial: false,
      rawDamage,
      element: baseAction.element,
      attackTraits: baseAction.attackTraits ?? [],
      hasHaki,
    };
  });

  return addLog(
    next,
    playerId,
    `Capitaine ${def.name} attaque avec ${baseAction.name} ! (${rawDamage} degats)`
  );
}

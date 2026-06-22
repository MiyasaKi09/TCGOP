import { produce } from "immer";
import type { GameState, PlayerId, Slot, EntryEffect } from "@/types";
import { getCaptainDef, getCardDef } from "./cardRegistry";
import { canAfford, spendVolonte } from "./volonte";
import { addLog, getOpponent, checkWinCondition } from "./gameState";
import { getBoardCharacters, getEffectiveAtk, getEffectiveDef } from "./board";

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

  // Free flip if a Mugiwara ally was KO'd this turn (Luffy).
  if (condition.freeIfAllyKO && state.players[playerId].allyKOedThisTurn) return true;

  // Check auto-flip condition (allies <= N)
  // Only available from turn 4+ to prevent early abuse
  if (condition.autoIfAlliesLte !== undefined && state.turnNumber >= 4) {
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
  const freeFlip =
    (condition.freeIfAllyKO && state.players[playerId].allyKOedThisTurn) ||
    (condition.autoIfAlliesLte !== undefined &&
      state.turnNumber >= 4 &&
      allyCount <= condition.autoIfAlliesLte);

  if (!freeFlip) {
    cost = condition.cost ?? 0;
    if (!canAfford(state, playerId, cost)) {
      throw new Error("Cannot afford captain flip");
    }
  }

  let next = cost > 0 ? spendVolonte(state, playerId, cost) : state;

  // Flip captain — damage already marked on the recto face carries over to the verso
  // face (Rulebook v3.1 §2.1: the face/PV max changes, marked damage stays).
  next = produce(next, (draft) => {
    const cap = draft.players[playerId].captain;
    cap.flipped = true;
    const dmgMarked = Math.max(0, def.recto.pv - cap.currentPv);
    cap.currentPv = def.verso.pv - dmgMarked;
    cap.slot = slot;
    cap.deployedTurn = draft.turnNumber;
  });

  next = addLog(
    next,
    playerId,
    `${def.name} s'engage sur le champ de bataille ! (verso, slot ${slot})`
  );

  // Flipping a badly wounded captain into a lower-PV face can be lethal.
  const flipWinner = checkWinCondition(next);
  if (flipWinner) {
    return produce(next, (draft) => {
      draft.winner = flipWinner;
    });
  }

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
        next = addLog(next, playerId, `Effet d'entree : ${effect.amount} degats a toute la Ligne Avant ennemie !`);
      } else if (effect.target === "single") {
        // Gear 2: hit the strongest enemy front char, else any char, else the captain.
        const enemies = getBoardCharacters(next, opponentId);
        let targetId: string | null = null;
        const front = enemies.filter((c) => c.slot && ["V1", "V2", "V3"].includes(c.slot));
        const pool = front.length > 0 ? front : enemies;
        for (const c of pool) {
          if (targetId === null || getEffectiveAtk(next, c.instanceId) > getEffectiveAtk(next, targetId)) targetId = c.instanceId;
        }
        if (targetId) {
          const tid = targetId;
          next = produce(next, (draft) => { draft.cards[tid].currentPv -= effect.amount; });
          const td = getCardDef(next.cards[tid].defId);
          next = addLog(next, playerId, `Effet d'entree (Gear 2) : ${effect.amount} degats a ${td.name} !`);
          if (next.cards[tid].currentPv <= 0) {
            const { removeFromBoard } = require("./board");
            const { grantKOBonus } = require("./volonte");
            const { applyOnKOEffects } = require("./passives");
            const koDefId = next.cards[tid].defId;
            next = addLog(next, opponentId, `${td.name} est KO !`);
            next = grantKOBonus(next, opponentId);
            next = removeFromBoard(next, tid);
            next = applyOnKOEffects(next, opponentId, playerId, koDefId);
          }
        } else {
          next = produce(next, (draft) => { draft.players[opponentId].captain.currentPv -= effect.amount; });
          next = addLog(next, playerId, `Effet d'entree (Gear 2) : ${effect.amount} degats au Capitaine !`);
        }
      }
      break;
    }

    case "grantSelfRush": {
      // Clear the captain's summoning sickness so it can act the turn it flips (Gear 2).
      next = produce(next, (draft) => { draft.players[playerId].captain.deployedTurn = -1; });
      next = addLog(next, playerId, `Effet d'entree : Gear 2 — le Capitaine peut agir immédiatement (Rush).`);
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
    targetDefVal = getEffectiveDef(state, targetInstanceId);
  }

  const rawDamage = Math.max(0, atk - targetDefVal);

  const hasHaki =
    ((def.verso.naturalHaki && def.verso.naturalHaki.length > 0) ?? false) ||
    state.turnNumber >= 7;

  let next = produce(state, (draft) => {
    const cap = draft.players[playerId].captain;
    cap.tapped = true;
    // One action per turn (Rulebook v3.1 §2.2/§6).
    cap.usedBaseAction = true;
    cap.usedSpecialAttack = true;
    draft.pendingAttack = {
      attackerId: `captain_${playerId}`,
      targetId: targetInstanceId,
      targetIsCaptain,
      isSpecial: false,
      rawDamage,
      attackPower: atk,
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

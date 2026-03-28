import { produce } from "immer";
import type { GameState, PlayerId } from "@/types";
import { VOLONTE_CAP, ALLY_KO_BONUS_VOL } from "./utils";

/**
 * Gain Volonte at the start of a turn.
 * Amount = turn number (cap 10).
 * Previous Vol. is lost — replaced by the new amount.
 */
export function gainVolonte(state: GameState): GameState {
  return produce(state, (draft) => {
    const player = draft.players[draft.currentPlayer];
    const amount = Math.min(draft.turnNumber, VOLONTE_CAP);
    player.volonte = amount;
  });
}

/**
 * Spend Volonte. Throws if insufficient.
 */
export function spendVolonte(
  state: GameState,
  playerId: PlayerId,
  amount: number
): GameState {
  if (amount <= 0) return state;
  const player = state.players[playerId];
  if (player.volonte < amount) {
    throw new Error(
      `Not enough Volonte: has ${player.volonte}, needs ${amount}`
    );
  }
  return produce(state, (draft) => {
    draft.players[playerId].volonte -= amount;
  });
}

/**
 * Check if a player can afford a cost.
 */
export function canAfford(
  state: GameState,
  playerId: PlayerId,
  cost: number
): boolean {
  return state.players[playerId].volonte >= cost;
}

/**
 * Grant bonus Volonte when an ally is KO'd (+2).
 */
export function grantKOBonus(
  state: GameState,
  playerId: PlayerId
): GameState {
  return produce(state, (draft) => {
    draft.players[playerId].volonte += ALLY_KO_BONUS_VOL;
  });
}

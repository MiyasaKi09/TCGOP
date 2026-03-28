import { produce } from "immer";
import type { GameState, PlayerId, HakiType } from "@/types";
import { addLog } from "./gameState";

/** Haki unlock thresholds */
const HAKI_THRESHOLDS: Record<HakiType, number> = {
  observation: 5,
  armament: 7,
  king: 10,
};

/** Check if a Haki type is available this turn */
export function isHakiAvailable(
  state: GameState,
  playerId: PlayerId,
  hakiType: HakiType
): boolean {
  const player = state.players[playerId];
  if (state.turnNumber < HAKI_THRESHOLDS[hakiType]) return false;

  switch (hakiType) {
    case "observation":
      return !player.observationUsed;
    case "armament":
      return !player.armamentUsed;
    case "king":
      return !player.kingUsed;
  }
}

/** Use Observation Haki: dodge 1 attack (called during counter window) */
export function useObservationHaki(
  state: GameState,
  playerId: PlayerId
): GameState {
  if (!isHakiAvailable(state, playerId, "observation")) {
    throw new Error("Observation Haki not available");
  }
  if (!state.pendingAttack) throw new Error("No pending attack to dodge");

  let next = produce(state, (draft) => {
    draft.players[playerId].observationUsed = true;
    // Cancel the pending attack
    draft.pendingAttack = null;
  });

  return addLog(next, playerId, `Haki de l'Observation ! Attaque esquivee !`);
}

/** Use Armament Haki: buff next attack +2 ATK + Haki trait */
export function useArmamentHaki(
  state: GameState,
  playerId: PlayerId,
  attackerInstanceId: string
): GameState {
  if (!isHakiAvailable(state, playerId, "armament")) {
    throw new Error("Armament Haki not available");
  }

  let next = produce(state, (draft) => {
    draft.players[playerId].armamentUsed = true;
    const card = draft.cards[attackerInstanceId];
    if (card) {
      card.modifiers.push({
        id: `haki_arm_${Date.now()}`,
        stat: "atk",
        amount: 2,
        source: "armament_haki",
        duration: "turn",
      });
    }
  });

  return addLog(next, playerId, `Haki de l'Armement active ! +2 ATK et touche les Logia.`);
}

import { produce } from "immer";
import type { GameState, PlayerId, HakiType } from "@/types";
import { addLog, getOpponent } from "./gameState";
import { getBoardCharacters, getEffectiveDef, removeFromBoard } from "./board";
import { getCaptainDef } from "./cardRegistry";
import { grantKOBonus } from "./volonte";

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
      // Armament is a passive from T7+ (Rulebook v3.1 §7) — never "used".
      return false;
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
  if (state.pendingAttack.cannotBeDodged) {
    throw new Error("This attack cannot be dodged");
  }

  let next = produce(state, (draft) => {
    draft.players[playerId].observationUsed = true;
    // Cancel the pending attack
    draft.pendingAttack = null;
  });

  return addLog(next, playerId, `Haki de l'Observation ! Attaque esquivee !`);
}

/** Does this player control a Conqueror unit (board character or captain)? */
export function hasConquerorInPlay(state: GameState, playerId: PlayerId): boolean {
  const captain = state.players[playerId].captain;
  const capDef = getCaptainDef(captain.defId);
  const capTraits = captain.flipped ? capDef.verso.traits : capDef.traits;
  if (capTraits?.includes("conqueror")) return true;
  if (capDef.traits?.includes("conqueror")) return true;

  return getBoardCharacters(state, playerId).some((c) => {
    const def = require("./cardRegistry").getCardDef(c.defId);
    return def.traits?.includes("conqueror");
  });
}

/**
 * Use Roi Haki (T10+): KO every enemy character with effective DEF <= 3. 1x/game.
 * Requires a Conqueror unit in play (Rulebook v3.1 §7).
 */
export function useKingHaki(state: GameState, playerId: PlayerId): GameState {
  if (!isHakiAvailable(state, playerId, "king")) {
    throw new Error("Roi Haki not available");
  }
  if (!hasConquerorInPlay(state, playerId)) {
    throw new Error("Roi Haki requires a Conquerant unit in play");
  }

  const opponentId = getOpponent(playerId);
  let next = produce(state, (draft) => {
    draft.players[playerId].kingUsed = true;
  });
  next = addLog(next, playerId, `👑 Haki des Rois ! Tous les ennemis DEF ≤ 3 sont KO !`);

  const { applyOnKOEffects } = require("./passives");
  // Snapshot victims first (board mutates as we remove them).
  const victims = getBoardCharacters(next, opponentId)
    .filter((c) => getEffectiveDef(next, c.instanceId) <= 3)
    .map((c) => ({ id: c.instanceId, defId: c.defId, owner: c.owner }));

  for (const v of victims) {
    const card = next.cards[v.id];
    if (!card || card.zone !== "board") continue;
    next = addLog(next, v.owner, `${require("./cardRegistry").getCardDef(v.defId).name} est KO (Haki des Rois) !`);
    next = grantKOBonus(next, v.owner);
    next = removeFromBoard(next, v.id);
    next = applyOnKOEffects(next, v.owner, playerId, v.defId);
  }

  return next;
}

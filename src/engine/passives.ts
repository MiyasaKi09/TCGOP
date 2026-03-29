import { produce } from "immer";
import type { GameState, PlayerId, Slot, PassiveEffect, AllyFilter } from "@/types";
import { getCardDef, getCaptainDef } from "./cardRegistry";
import { getBoardCharacters, getAdjacentSlots, getSlotOf } from "./board";
import { addLog, getOpponent } from "./gameState";
import { ALL_SLOTS } from "./utils";

// ============================================================
// Start-of-turn passive effects
// ============================================================

/**
 * Apply all start-of-turn passives for the current player.
 * Called during startTurn after untap/draw/volonte.
 */
export function applyStartOfTurnPassives(
  state: GameState,
  playerId: PlayerId
): GameState {
  let next = state;
  const player = state.players[playerId];

  // Process each board character's passives
  for (const slot of ALL_SLOTS) {
    const charId = player.board[slot as Slot];
    if (!charId) continue;
    const card = state.cards[charId];
    if (!card) continue;
    const def = getCardDef(card.defId);
    if (!def.passive) continue;

    for (const effect of def.passive.effects) {
      next = resolveStartOfTurnEffect(next, playerId, charId, slot as Slot, effect);
    }
  }

  // Process captain passives (recto or verso)
  next = applyCaptainStartOfTurn(next, playerId);

  return next;
}

function resolveStartOfTurnEffect(
  state: GameState,
  playerId: PlayerId,
  sourceId: string,
  sourceSlot: Slot,
  effect: PassiveEffect
): GameState {
  switch (effect.type) {
    case "healAdjacent": {
      const adjacent = getAdjacentSlots(sourceSlot);
      const player = state.players[playerId];
      const sourceDef = getCardDef(state.cards[sourceId].defId);
      return produce(state, (draft) => {
        for (const adjSlot of adjacent) {
          const adjId = player.board[adjSlot as Slot];
          if (!adjId) continue;
          const adjCard = draft.cards[adjId];
          if (!adjCard) continue;
          const adjDef = getCardDef(adjCard.defId);
          const maxPv = adjDef.pv ?? adjCard.currentPv;
          adjCard.currentPv = Math.min(adjCard.currentPv + effect.amount, maxPv);
        }
        draft.log.push({
          turn: draft.turnNumber,
          player: playerId,
          message: `${sourceDef.name} soigne ${effect.amount} PV aux adjacents`,
        });
      });
    }
    default:
      return state;
  }
}

function applyCaptainStartOfTurn(
  state: GameState,
  playerId: PlayerId
): GameState {
  // Captain passives that trigger at start of turn are rare
  // Most captain passives are continuous (handled by recalculatePassiveBuffs)
  return state;
}

// ============================================================
// Recalculate all passive buffs (continuous effects)
// Called after deploy, KO, equip, captain flip
// ============================================================

/**
 * Remove old passive modifiers and reapply all continuous passive buffs.
 */
export function recalculatePassiveBuffs(
  state: GameState,
  playerId: PlayerId
): GameState {
  return produce(state, (draft) => {
    const player = draft.players[playerId];

    // Remove all passive-source modifiers from all board characters
    for (const slot of ALL_SLOTS) {
      const charId = player.board[slot as Slot];
      if (!charId) continue;
      const card = draft.cards[charId];
      if (card) {
        card.modifiers = card.modifiers.filter(
          (m) => !m.source.startsWith("passive_") && !m.source.startsWith("captain_") && !m.source.startsWith("synergy_")
        );
      }
    }

    // Collect all board characters
    const boardChars: { id: string; defId: string; slot: Slot }[] = [];
    for (const slot of ALL_SLOTS) {
      const charId = player.board[slot as Slot];
      if (charId && draft.cards[charId]) {
        boardChars.push({ id: charId, defId: draft.cards[charId].defId, slot: slot as Slot });
      }
    }

    // === Captain passive buffs ===
    const captain = player.captain;
    const capDef = getCaptainDef(captain.defId);
    const capPassive = captain.flipped ? capDef.verso.passive : capDef.recto.passive;

    for (const effect of capPassive.effects) {
      if (effect.type === "buffAlly") {
        for (const char of boardChars) {
          if (matchesFilter(draft, char.defId, effect.filter)) {
            draft.cards[char.id].modifiers.push({
              id: `captain_${effect.stat}_${char.id}`,
              stat: effect.stat,
              amount: effect.amount,
              source: `captain_${capDef.id}`,
              duration: "permanent",
            });
          }
        }
      }
    }

    // === Character passive buffs (buffAlly on other characters) ===
    for (const char of boardChars) {
      const def = getCardDef(char.defId);
      if (!def.passive) continue;
      for (const effect of def.passive.effects) {
        if (effect.type === "buffAlly") {
          for (const target of boardChars) {
            if (target.id === char.id) continue; // Don't buff self
            if (matchesFilter(draft, target.defId, effect.filter)) {
              draft.cards[target.id].modifiers.push({
                id: `passive_${char.id}_${effect.stat}_${target.id}`,
                stat: effect.stat,
                amount: effect.amount,
                source: `passive_${char.id}`,
                duration: "permanent",
              });
            }
          }
        }
      }
    }

    // === Ship passive buffs ===
    if (player.activeShip) {
      const shipCard = draft.cards[player.activeShip];
      if (shipCard) {
        const shipDef = getCardDef(shipCard.defId);
        if (shipDef.shipPassive) {
          const desc = shipDef.shipPassive.toLowerCase();
          // Parse common ship passive patterns
          const atkMatch = desc.match(/\+(\d+)\s*atk/i);
          const defMatch = desc.match(/\+(\d+)\s*def/i);
          const pvMatch = desc.match(/\+(\d+)\s*pv/i);
          const shipAtkBonus = atkMatch ? parseInt(atkMatch[1]) : 0;
          const shipDefBonus = defMatch ? parseInt(defMatch[1]) : 0;

          for (const char of boardChars) {
            // Check faction filter (e.g. "Mugiwara" or "Marines")
            const charDef = getCardDef(char.defId);
            const factionMatch =
              (desc.includes("mugiwara") && charDef.tags?.includes("mugiwara")) ||
              (desc.includes("marine") && charDef.tags?.includes("marine")) ||
              (!desc.includes("mugiwara") && !desc.includes("marine"));

            if (factionMatch) {
              if (shipAtkBonus > 0) {
                draft.cards[char.id].modifiers.push({
                  id: `ship_passive_atk_${char.id}`,
                  stat: "atk",
                  amount: shipAtkBonus,
                  source: `passive_ship_${shipDef.id}`,
                  duration: "permanent",
                });
              }
              if (shipDefBonus > 0) {
                draft.cards[char.id].modifiers.push({
                  id: `ship_passive_def_${char.id}`,
                  stat: "def",
                  amount: shipDefBonus,
                  source: `passive_ship_${shipDef.id}`,
                  duration: "permanent",
                });
              }
            }
          }
        }
      }
    }

    // === Synergy bonuses ===
    for (const char of boardChars) {
      const def = getCardDef(char.defId);
      if (!def.synergies) continue;
      for (const syn of def.synergies) {
        // Check if partner is on the board
        const partnerOnBoard = boardChars.some((c) => c.defId === syn.partnerId);
        if (partnerOnBoard) {
          draft.cards[char.id].modifiers.push({
            id: `synergy_${char.id}_${syn.partnerId}`,
            stat: "atk",
            amount: syn.atkBonus,
            source: `synergy_${syn.partnerId}`,
            duration: "permanent",
          });
        }
      }
    }
  });
}

function matchesFilter(
  draft: GameState,
  defId: string,
  filter?: AllyFilter
): boolean {
  if (!filter) return true;
  const def = getCardDef(defId);
  if (filter.faction && def.faction !== filter.faction) return false;
  if (filter.tag && !(def.tags?.includes(filter.tag))) return false;
  if (filter.trait && !(def.traits?.includes(filter.trait))) return false;
  return true;
}

// ============================================================
// On-KO effects
// ============================================================

/**
 * Handle effects that trigger when a character is KO'd.
 */
export function applyOnKOEffects(
  state: GameState,
  koPlayerId: PlayerId,
  killerPlayerId: PlayerId,
  koDefId: string
): GameState {
  let next = state;
  const koPlayer = state.players[koPlayerId];

  // Captain onAllyKO passive
  const capDef = getCaptainDef(koPlayer.captain.defId);
  const capPassive = koPlayer.captain.flipped ? capDef.verso.passive : capDef.recto.passive;

  for (const effect of capPassive.effects) {
    if (effect.type === "onAllyKO" && effect.effect === "bonusWill") {
      next = produce(next, (draft) => {
        draft.players[koPlayerId].volonte += effect.amount;
        draft.log.push({
          turn: draft.turnNumber,
          player: koPlayerId,
          message: `Passif Capitaine : +${effect.amount} Vol. (allie KO)`,
        });
      });
    }
  }

  // Synergy rage bonus (partner KO'd → surviving partner gets temp ATK)
  const boardChars = getBoardCharacters(next, koPlayerId);
  for (const char of boardChars) {
    const def = getCardDef(char.defId);
    if (!def.synergies) continue;
    for (const syn of def.synergies) {
      if (syn.partnerId === koDefId && syn.onPartnerKO) {
        next = produce(next, (draft) => {
          draft.cards[char.instanceId].modifiers.push({
            id: `synrage_${char.instanceId}_${Date.now()}`,
            stat: "atk",
            amount: syn.onPartnerKO!,
            source: `synergy_rage_${koDefId}`,
            duration: "turn",
          });
          draft.log.push({
            turn: draft.turnNumber,
            player: koPlayerId,
            message: `${def.name} : rage ! +${syn.onPartnerKO} ATK (${getCardDef(koDefId).name} KO)`,
          });
        });
      }
    }
  }

  // Recalculate passive buffs (someone left the board)
  next = recalculatePassiveBuffs(next, koPlayerId);

  return next;
}

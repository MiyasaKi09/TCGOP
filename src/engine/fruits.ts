import { produce } from "immer";
import type { GameState, PlayerId, Trait } from "@/types";
import { getCardDef } from "./cardRegistry";
import { addLog } from "./gameState";
import { canAfford, spendVolonte } from "./volonte";
import { recalculatePassiveBuffs } from "./passives";

/**
 * Apply Devil Fruit base effects when equipped.
 * Called from equipObject after the fruit is attached.
 */
export function applyFruitBaseEffects(
  state: GameState,
  fruitInstanceId: string,
  bearerInstanceId: string
): GameState {
  const fruitCard = state.cards[fruitInstanceId];
  if (!fruitCard) return state;
  const fruitDef = getCardDef(fruitCard.defId);
  if (!fruitDef.fruitEffects) return state;

  const base = fruitDef.fruitEffects.base;
  const bearerDef = getCardDef(state.cards[bearerInstanceId].defId);

  let next = state;

  // Apply granted traits via modifiers
  if (base.grantsTraits) {
    next = produce(next, (draft) => {
      const bearer = draft.cards[bearerInstanceId];
      // Add ATK/DEF bonuses
      if (base.atkBonus) {
        bearer.modifiers.push({
          id: `fruit_atk_${fruitInstanceId}`,
          stat: "atk",
          amount: base.atkBonus,
          source: `fruit_${fruitDef.id}`,
          duration: "permanent",
        });
      }
      if (base.defBonus) {
        bearer.modifiers.push({
          id: `fruit_def_${fruitInstanceId}`,
          stat: "def",
          amount: base.defBonus,
          source: `fruit_${fruitDef.id}`,
          duration: "permanent",
        });
      }
    });
  }

  next = addLog(
    next,
    fruitCard.owner,
    `${bearerDef.name} mange le ${fruitDef.name} ! ${base.passiveDescription ?? ""}`
  );

  return next;
}

/**
 * Check if a Devil Fruit can be awakened.
 */
export function canAwakenFruit(
  state: GameState,
  playerId: PlayerId,
  fruitInstanceId: string
): boolean {
  const fruitCard = state.cards[fruitInstanceId];
  if (!fruitCard || fruitCard.isAwakened) return false;

  const fruitDef = getCardDef(fruitCard.defId);
  if (!fruitDef.fruitEffects?.awakening) return false;

  const awakening = fruitDef.fruitEffects.awakening;

  // Check bearer is the legitimate one
  // Find which character has this fruit equipped
  const bearer = Object.values(state.cards).find(
    (c) => c.zone === "board" && c.owner === playerId && c.attachedObjects.includes(fruitInstanceId)
  );
  if (!bearer) return false;

  const bearerDef = getCardDef(bearer.defId);
  if (awakening.porteurLegitime && !bearerDef.name.includes(awakening.porteurLegitime)) {
    return false;
  }

  // Check minimum turns
  if (state.turnNumber < awakening.minTurns) return false;

  // Check Vol cost
  if (!canAfford(state, playerId, awakening.volCost)) return false;

  return true;
}

/**
 * Awaken a Devil Fruit (irreversible).
 */
export function awakenFruit(
  state: GameState,
  playerId: PlayerId,
  fruitInstanceId: string
): GameState {
  if (!canAwakenFruit(state, playerId, fruitInstanceId)) {
    throw new Error("Cannot awaken this fruit");
  }

  const fruitCard = state.cards[fruitInstanceId];
  const fruitDef = getCardDef(fruitCard.defId);
  const awakening = fruitDef.fruitEffects!.awakening!;

  // Find bearer
  const bearer = Object.values(state.cards).find(
    (c) => c.zone === "board" && c.owner === playerId && c.attachedObjects.includes(fruitInstanceId)
  );
  if (!bearer) throw new Error("No bearer found");

  let next = spendVolonte(state, playerId, awakening.volCost);

  next = produce(next, (draft) => {
    // Mark as awakened
    draft.cards[fruitInstanceId].isAwakened = true;

    // Apply awakening bonuses
    const b = draft.cards[bearer.instanceId];
    if (awakening.atkBonus) {
      b.modifiers.push({
        id: `fruit_awaken_atk_${fruitInstanceId}`,
        stat: "atk",
        amount: awakening.atkBonus,
        source: `fruit_awaken_${fruitDef.id}`,
        duration: "permanent",
      });
    }
    if (awakening.defBonus) {
      b.modifiers.push({
        id: `fruit_awaken_def_${fruitInstanceId}`,
        stat: "def",
        amount: awakening.defBonus,
        source: `fruit_awaken_${fruitDef.id}`,
        duration: "permanent",
      });
    }
  });

  const bearerDef = getCardDef(bearer.defId);
  next = addLog(
    next,
    playerId,
    `⭐ EVEIL ! ${bearerDef.name} eveille le ${fruitDef.name} ! ${awakening.passiveDescription ?? ""}`
  );

  next = recalculatePassiveBuffs(next, playerId);

  return next;
}

/**
 * Get the traits granted by equipped fruits on a character.
 */
export function getFruitTraits(
  state: GameState,
  instanceId: string
): Trait[] {
  const card = state.cards[instanceId];
  if (!card) return [];

  const traits: Trait[] = [];
  for (const objId of card.attachedObjects) {
    const objCard = state.cards[objId];
    if (!objCard) continue;
    const objDef = getCardDef(objCard.defId);
    if (objDef.subtype !== "fruit" || !objDef.fruitEffects) continue;

    const base = objDef.fruitEffects.base;
    if (base.grantsTraits) {
      traits.push(...base.grantsTraits);
    }

    // If awakened, add awakening traits
    if (objCard.isAwakened && objDef.fruitEffects.awakening?.grantsTraits) {
      traits.push(...objDef.fruitEffects.awakening.grantsTraits);
    }
  }

  return traits;
}

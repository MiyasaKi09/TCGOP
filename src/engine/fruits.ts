import { produce } from "immer";
import type { GameState, PlayerId, Trait } from "@/types";
import { getCardDef, getCaptainDef } from "./cardRegistry";
import { addLog } from "./gameState";
import { canAfford, spendVolonte } from "./volonte";
import { recalculatePassiveBuffs } from "./passives";

/**
 * Who is wearing a fruit — decision §8.28 (follow-up) added the second
 * possibility. Rust: `fruits::FruitBearer`.
 */
export type FruitBearer =
  /** A board character, the only bearer the engine knew before §8.28. */
  | { kind: "character"; instanceId: string }
  /** The player's own captain, the printed bearer of the three signature SR
   *  fruits ("Équipable sur Luffy / Crocodile / Akainu"). */
  | { kind: "captain"; playerId: PlayerId };

/**
 * The bearer search widened to the captain (decision §8.28 follow-up). The
 * character scan comes first and is unchanged, so nothing about the existing
 * bearers moves; a fruit can only ever be attached to one unit.
 * Rust: `fruits::find_fruit_bearer`.
 */
export function findFruitBearer(
  state: GameState,
  playerId: PlayerId,
  fruitInstanceId: string
): FruitBearer | null {
  const char = Object.values(state.cards).find(
    (c) => c.zone === "board" && c.owner === playerId && c.attachedObjects.includes(fruitInstanceId)
  );
  if (char) return { kind: "character", instanceId: char.instanceId };
  if ((state.players[playerId].captain.attachedObjects ?? []).includes(fruitInstanceId)) {
    return { kind: "captain", playerId };
  }
  return null;
}

/** The printed name of a bearer, for the `porteurLegitime` test and the logs. */
function bearerName(state: GameState, bearer: FruitBearer): string {
  return bearer.kind === "character"
    ? getCardDef(state.cards[bearer.instanceId].defId).name
    : getCaptainDef(state.players[bearer.playerId].captain.defId).name;
}

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
 * Decision §8.28 (follow-up) — `applyFruitBaseEffects` for a **captain** bearer.
 *
 * The same two modifiers with the same ids and the same source, pushed onto
 * `captain.modifiers` (which `recalculatePassiveBuffs` never strips — it only
 * rebuilds the `passive_` / `captain_` / `synergy_` modifiers of board
 * characters), and the same log line with the captain's name. The granted
 * traits are read from the attachment itself by `captainHasTraitNow`, exactly
 * as `hasTrait` reads a character's.
 *
 * Like the Rust reference the two bonuses are pushed independently of
 * `grantsTraits` (§8.1 item 3); latent on the shipped catalogue, where every
 * fruit declares `grantsTraits`. Rust: `fruits::apply_fruit_base_effects_on_captain`.
 */
export function applyFruitBaseEffectsOnCaptain(
  state: GameState,
  fruitInstanceId: string,
  playerId: PlayerId
): GameState {
  const fruitCard = state.cards[fruitInstanceId];
  if (!fruitCard) return state;
  const fruitDef = getCardDef(fruitCard.defId);
  if (!fruitDef.fruitEffects) return state;

  const base = fruitDef.fruitEffects.base;
  const capName = getCaptainDef(state.players[playerId].captain.defId).name;

  let next = produce(state, (draft) => {
    const cap = draft.players[playerId].captain;
    // A zero bonus pushes no modifier (a real "no bonus"), like the character path.
    if (base.atkBonus) {
      cap.modifiers.push({
        id: `fruit_atk_${fruitInstanceId}`,
        stat: "atk",
        amount: base.atkBonus,
        source: `fruit_${fruitDef.id}`,
        duration: "permanent",
      });
    }
    if (base.defBonus) {
      cap.modifiers.push({
        id: `fruit_def_${fruitInstanceId}`,
        stat: "def",
        amount: base.defBonus,
        source: `fruit_${fruitDef.id}`,
        duration: "permanent",
      });
    }
  });

  next = addLog(
    next,
    playerId,
    `${capName} mange le ${fruitDef.name} ! ${base.passiveDescription ?? ""}`
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

  // Check bearer is the legitimate one. Decision §8.28 (follow-up): the bearer
  // may be the captain — `porteurLegitime` is "Luffy" / "Crocodile" / "Akainu"
  // on the three signature fruits, i.e. exactly the captains that now wear
  // them, and the check is the same `includes` on the bearer's printed name.
  const bearer = findFruitBearer(state, playerId, fruitInstanceId);
  if (!bearer) return false;

  const name = bearerName(state, bearer);
  if (awakening.porteurLegitime && !name.includes(awakening.porteurLegitime)) {
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

  // Find bearer — decision §8.28 (follow-up): a captain is one.
  const bearer = findFruitBearer(state, playerId, fruitInstanceId);
  if (!bearer) throw new Error("No bearer found");
  const name = bearerName(state, bearer);

  let next = spendVolonte(state, playerId, awakening.volCost);

  next = produce(next, (draft) => {
    // Mark as awakened
    draft.cards[fruitInstanceId].isAwakened = true;

    // Apply awakening bonuses — the same two modifiers, on whichever unit
    // wears the fruit.
    const mods =
      bearer.kind === "character"
        ? draft.cards[bearer.instanceId].modifiers
        : draft.players[bearer.playerId].captain.modifiers;
    if (awakening.atkBonus) {
      mods.push({
        id: `fruit_awaken_atk_${fruitInstanceId}`,
        stat: "atk",
        amount: awakening.atkBonus,
        source: `fruit_awaken_${fruitDef.id}`,
        duration: "permanent",
      });
    }
    if (awakening.defBonus) {
      mods.push({
        id: `fruit_awaken_def_${fruitInstanceId}`,
        stat: "def",
        amount: awakening.defBonus,
        source: `fruit_awaken_${fruitDef.id}`,
        duration: "permanent",
      });
    }
  });

  next = addLog(
    next,
    playerId,
    `⭐ EVEIL ! ${name} eveille le ${fruitDef.name} ! ${awakening.passiveDescription ?? ""}`
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

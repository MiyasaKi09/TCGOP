import { produce } from "immer";
import type {
  GameState,
  PlayerId,
  Slot,
  CardInstance,
  Trait,
} from "@/types";
import type { CardDef, CaptainDef, ObjectSubtype, AttackTrait } from "@/types";
import { getCardDef, getCaptainDef } from "./cardRegistry";
import { spendVolonte, canAfford } from "./volonte";
import { addLog, getOpponent } from "./gameState";
import { ADJACENCY, FRONT_SLOTS, BACK_SLOTS, ALL_SLOTS } from "./utils";

// ============================================================
// Queries
// ============================================================

/**
 * Decision §8.37 (`betrayal`, BW-024) — who *controls* a body right now.
 *
 * `BW-024` Trahison lends an enemy character for one turn: the loan writes
 * `controlledBy`, never `owner`, "so KO bonuses and win conditions keep
 * pointing at the original owner". Every turn-scoped question (who may attack
 * with it, whose enemies are its targets, which board cell holds it, who may
 * move it) reads this; every ownership question (graveyard, KO bonus, win
 * condition, equipment) keeps reading `owner`.
 * Rust: `CardInstance::controller()`.
 */
export function controllerOf(card: CardInstance): PlayerId {
  return card.controlledBy ?? card.owner;
}

/**
 * Decision §8.37 (`embargo`, MR-027) — "L'adversaire ne peut ni équiper ni
 * jouer de Navire à son prochain tour." Rust: `PlayerState::is_embargoed()`.
 */
export function isEmbargoed(state: GameState, playerId: PlayerId): boolean {
  return (state.players[playerId].embargoTurns ?? 0) > 0;
}

/** Get all characters on the board for a player */
export function getBoardCharacters(
  state: GameState,
  playerId: PlayerId
): CardInstance[] {
  const player = state.players[playerId];
  const result: CardInstance[] = [];
  for (const slot of ALL_SLOTS) {
    const id = player.board[slot];
    if (id) {
      const card = state.cards[id];
      if (card) result.push(card);
    }
  }
  return result;
}

/** Get character in a specific slot */
export function getCharacterInSlot(
  state: GameState,
  playerId: PlayerId,
  slot: Slot
): CardInstance | null {
  const id = state.players[playerId].board[slot];
  return id ? state.cards[id] ?? null : null;
}

/** Get empty slots for a player */
export function getEmptySlots(
  state: GameState,
  playerId: PlayerId
): Slot[] {
  const player = state.players[playerId];
  return ALL_SLOTS.filter((s) => player.board[s] === null) as Slot[];
}

/** Get the slot of a card instance on the board */
export function getSlotOf(
  state: GameState,
  instanceId: string
): Slot | null {
  const card = state.cards[instanceId];
  if (!card || card.zone !== "board") return null;
  return card.slot ?? null;
}

/** Check if a slot is in the front row */
export function isFrontSlot(slot: Slot): boolean {
  return (FRONT_SLOTS as readonly string[]).includes(slot);
}

/** Check if a slot is in the back row */
export function isBackSlot(slot: Slot): boolean {
  return (BACK_SLOTS as readonly string[]).includes(slot);
}

/** Get adjacent slots */
export function getAdjacentSlots(slot: Slot): Slot[] {
  return (ADJACENCY[slot] ?? []) as Slot[];
}

/** Check if a player has any front-row characters (including verso captain) */
export function hasFrontRow(
  state: GameState,
  playerId: PlayerId
): boolean {
  const player = state.players[playerId];
  const hasCharInFront = FRONT_SLOTS.some((s) => player.board[s] !== null);
  // Captain verso in front row also counts
  if (player.captain.flipped && player.captain.slot && isFrontSlot(player.captain.slot)) {
    return true;
  }
  return hasCharInFront;
}

/** Get the effective ATK of a character (base + equipment + modifiers) */
export function getEffectiveAtk(
  state: GameState,
  instanceId: string
): number {
  const card = state.cards[instanceId];
  if (!card) return 0;
  const def = getCardDef(card.defId);
  let atk = def.atk ?? 0;

  // Equipment bonuses
  for (const objId of card.attachedObjects) {
    const objCard = state.cards[objId];
    if (objCard) {
      const objDef = getCardDef(objCard.defId);
      atk += objDef.bonusAtk ?? 0;
    }
  }

  // Modifier bonuses
  for (const mod of card.modifiers) {
    if (mod.stat === "atk") atk += mod.amount;
  }

  return Math.max(0, atk);
}

/** Get the effective DEF of a character */
export function getEffectiveDef(
  state: GameState,
  instanceId: string
): number {
  const card = state.cards[instanceId];
  if (!card) return 0;
  const def = getCardDef(card.defId);
  let defVal = def.def ?? 0;

  // Equipment bonuses
  for (const objId of card.attachedObjects) {
    const objCard = state.cards[objId];
    if (objCard) {
      const objDef = getCardDef(objCard.defId);
      defVal += objDef.bonusDef ?? 0;
    }
  }

  // Modifier bonuses
  for (const mod of card.modifiers) {
    if (mod.stat === "def") defVal += mod.amount;
  }

  return Math.max(0, defVal);
}

/**
 * Does any object of `attached` grant `trait` to whoever wears it?
 *
 * The object half of `hasTrait`, extracted so the captain reads its equipment
 * with exactly the same rule (decision §8.28 follow-up): a fruit's
 * `base.grantsTraits`, and its `awakening.grantsTraits` once awakened.
 * Rust: `board::attachments_grant_trait`.
 */
export function attachmentsGrantTrait(
  state: GameState,
  attached: readonly string[],
  trait: Trait
): boolean {
  for (const objId of attached) {
    const objCard = state.cards[objId];
    if (!objCard) continue;
    const objDef = getCardDef(objCard.defId);
    // Decision §8.47 : `CardDef.grantsTraits` est lu. Le champ TS est une union
    // `(Trait | AttackTrait)[]` là où le Rust porte un `GrantedTrait` à deux
    // bras ; la seule lecture déterministe côté TS est « la valeur est-elle un
    // `Trait` ? » — ce qui reproduit exactement les données livrées, où les
    // trois fusils `RH-011/012/013` portent `GrantedTrait::Trait(Trait::Range)`.
    // Ici `trait` est typé `Trait`, donc l'appartenance suffit : une entrée
    // purement `AttackTrait` (zone / total / impact) ne peut jamais l'égaler.
    if (objDef.grantsTraits?.includes(trait)) return true;
    if (objDef.fruitEffects?.base.grantsTraits?.includes(trait)) return true;
    if (objCard.isAwakened && objDef.fruitEffects?.awakening?.grantsTraits?.includes(trait)) return true;
  }
  return false;
}

/** Les huit `Trait` du jeu — le bras « trait du porteur » de `grantsTraits`. */
const ALL_TRAITS = ["shield", "range", "stealth", "rush", "cursed", "logia", "piercing", "conqueror"] as const;

/**
 * Decision §8.47 — le bras `AttackTrait` de `grantsTraits` des objets portés :
 * les mots-clés qui décrivent l'*attaque* et non l'unité (`zone`, `total`,
 * `impact`), fusionnés dans les `attackTraits` de **chaque** déclaration du
 * porteur (base, spéciale, spéciale de fruit éveillé — et les mêmes pour un
 * capitaine porteur, §8.28). En ordre d'attachement, dédupliqués.
 * Rust: `board::attachments_granted_attack_traits`.
 */
export function attachmentsGrantedAttackTraits(
  state: GameState,
  attached: readonly string[]
): AttackTrait[] {
  const out: AttackTrait[] = [];
  for (const objId of attached) {
    const objCard = state.cards[objId];
    if (!objCard) continue;
    for (const g of getCardDef(objCard.defId).grantsTraits ?? []) {
      if ((ALL_TRAITS as readonly string[]).includes(g)) continue; // bras « trait du porteur »
      if (!out.includes(g as AttackTrait)) out.push(g as AttackTrait);
    }
  }
  return out;
}

/** [`attachmentsGrantedAttackTraits`] pour un personnage du plateau. */
export function grantedAttackTraits(state: GameState, instanceId: string): AttackTrait[] {
  const card = state.cards[instanceId];
  if (!card) return [];
  return attachmentsGrantedAttackTraits(state, card.attachedObjects);
}

/** Check if a character has a specific trait (including from Devil Fruits) */
export function hasTrait(
  state: GameState,
  instanceId: string,
  trait: Trait
): boolean {
  const card = state.cards[instanceId];
  if (!card) return false;
  const def = getCardDef(card.defId);
  if (def.traits?.includes(trait)) return true;

  // Check traits from equipped Devil Fruits
  return attachmentsGrantTrait(state, card.attachedObjects, trait);
}

/**
 * Decision §8.5 — the maximum PV of a board instance: the printed `def.pv`
 * minus its permanent max-PV loss. `undefined` when the definition has no `pv`.
 * Rust: `board::max_pv_of`.
 */
export function maxPvOf(state: GameState, instanceId: string): number | undefined {
  const card = state.cards[instanceId];
  if (!card) return undefined;
  const printed = getCardDef(card.defId).pv;
  if (printed === undefined) return undefined;
  return printed - (card.pvMaxLoss ?? 0);
}

/**
 * Decision §8.36/§8.38/§8.40 — the single "perd N PV permanent (Sable)" path for
 * a **character**: the maximum drops for good and the current PV follows it
 * down, so `healUnit` can never climb back over the loss. A non-positive loss,
 * a missing instance or one that already left the board is a no-op.
 * Rust: `board::apply_permanent_pv_loss`.
 */
export function applyPermanentPvLoss(
  state: GameState,
  instanceId: string,
  loss: number
): GameState {
  if (loss <= 0) return state;
  const card = state.cards[instanceId];
  if (!card || card.zone !== "board") return state;
  return produce(state, (draft) => {
    const c = draft.cards[instanceId];
    c.pvMaxLoss = (c.pvMaxLoss ?? 0) + loss;
    const printed = getCardDef(c.defId).pv;
    if (printed !== undefined) {
      const maxPv = printed - (c.pvMaxLoss ?? 0);
      if (c.currentPv > maxPv) c.currentPv = maxPv;
    }
  });
}

/**
 * Decision §8.40 — the captain counterpart of `applyPermanentPvLoss`: the same
 * rule against `CaptainInstance.pvMaxLoss`, with the maximum read from the
 * **active** face's printed PV. Rust: `board::apply_captain_permanent_pv_loss`.
 */
export function applyCaptainPermanentPvLoss(
  state: GameState,
  playerId: PlayerId,
  loss: number
): GameState {
  if (loss <= 0) return state;
  return produce(state, (draft) => {
    const cap = draft.players[playerId].captain;
    const capDef = getCaptainDef(cap.defId);
    cap.pvMaxLoss = (cap.pvMaxLoss ?? 0) + loss;
    const printed = cap.flipped ? capDef.verso.pv : capDef.recto.pv;
    const maxPv = printed - (cap.pvMaxLoss ?? 0);
    if (cap.currentPv > maxPv) cap.currentPv = maxPv;
  });
}

/**
 * Decision §8.5 — the single heal path: `new = min(current + amount, maxPv)`
 * then `new = max(new, current)`, so "Soigne N PV" can only ever add PV. A unit
 * carrying `noHeal` or `desiccation` is skipped entirely. Returns the PV
 * actually restored. Rust: `board::heal_unit`.
 */
export function healUnit(
  state: GameState,
  instanceId: string,
  amount: number
): { state: GameState; healed: number } {
  const card = state.cards[instanceId];
  if (!card) return { state, healed: 0 };
  if (card.statusEffects.some((e) => e.type === "noHeal" || e.type === "desiccation")) {
    return { state, healed: 0 };
  }
  const current = card.currentPv;
  // A definition without `pv` caps at the pre-heal PV (TS `d.pv ?? c.currentPv`).
  const maxPv = maxPvOf(state, instanceId) ?? current;
  const newPv = Math.max(Math.min(current + amount, maxPv), current);
  if (newPv === current) return { state, healed: 0 };
  return {
    state: produce(state, (draft) => {
      draft.cards[instanceId].currentPv = newPv;
    }),
    healed: newPv - current,
  };
}

/**
 * Decision §8.28 — how many objects of `subtype` a character may wear: 1, or
 * 3 / 2 weapons (`threeWeaponSlots`, `twoWeaponSlots`) and 2 accessories
 * (`twoAccessorySlots`). Rust: `board::max_object_slots`.
 */
export function maxObjectSlots(targetDef: CardDef, subtype: ObjectSubtype): number {
  const effects = targetDef.passive?.effects ?? [];
  if (subtype === "weapon") {
    if (effects.some((e) => e.type === "threeWeaponSlots")) return 3;
    if (effects.some((e) => e.type === "twoWeaponSlots")) return 2;
    return 1;
  }
  if (subtype === "accessory") {
    if (effects.some((e) => e.type === "twoAccessorySlots")) return 2;
    return 1;
  }
  return 1;
}

/** Decision §8.28 — has `target` still room for `objDef`? Rust: `board::has_free_object_slot`. */
export function hasFreeObjectSlot(
  state: GameState,
  target: CardInstance,
  objDef: CardDef
): boolean {
  if (!objDef.subtype) return true;
  const targetDef = getCardDef(target.defId);
  let same = 0;
  for (const id of target.attachedObjects) {
    const inst = state.cards[id];
    if (!inst) continue;
    if (getCardDef(inst.defId).subtype === objDef.subtype) same += 1;
  }
  return same < maxObjectSlots(targetDef, objDef.subtype);
}

/**
 * Decision §8.28 — may `targetDef` wear an object printed "Équipable sur
 * {restriction}"? The restriction is either a character **name** (a substring
 * of the printed name: "Zoro", "Mr. 4", "Nami") or a **tag** ("bretteur",
 * "tireur"). An empty restriction is JS-falsy and never checked.
 * Rust: `board::equip_restriction_ok` / `board::restriction_matches`.
 */
export function equipRestrictionOk(targetDef: CardDef, restriction: string): boolean {
  return restrictionMatches(targetDef.name, targetDef.tags ?? [], restriction);
}

/**
 * The name-or-tag half of `equipRestrictionOk`, shared with the captain bearer
 * (decision §8.28 follow-up): the printed restriction is satisfied by a
 * substring of the unit's **name** ("Zoro", "Mr. 4", "Luffy") or by one of its
 * **tags** ("bretteur", "tireur"). An empty restriction is JS-falsy and never
 * checked. Rust: `board::restriction_matches`.
 */
export function restrictionMatches(
  name: string,
  tags: readonly string[],
  restriction: string
): boolean {
  if (!restriction) return true;
  return name.includes(restriction) || tags.some((t) => t === restriction);
}

/**
 * Decision §8.28 (follow-up) — may this player's captain wear `objDef`?
 *
 * The three signature SR Devil Fruits are printed "Équipable sur Luffy",
 * "… sur Crocodile", "… sur Akainu", and those three names exist in the game
 * **only** as captains: no character in any set is called Luffy, Crocodile or
 * Akainu and no set carries a matching tag, so the character-only rule of
 * §8.28 left all three unequippable — three dead SR cards, one in each of
 * three shipped decklists, and with them the whole `BW-011` Ground Death
 * awakening. The natural — and literal — reading of the printed line is taken:
 * the fruit is worn by the captain it names.
 *
 * The rule is deliberately narrow so nothing else moves: an object reaches the
 * captain **only** when its own printed `restriction` names that captain by
 * name or tag, so an unrestricted object is refused and the rest of the
 * catalogue is untouched. Rust: `board::captain_equip_restriction_ok`.
 */
export function captainEquipRestrictionOk(capDef: CaptainDef, objDef: CardDef): boolean {
  if (!objDef.restriction) return false;
  return restrictionMatches(capDef.name, capDef.tags ?? [], objDef.restriction);
}

/**
 * Decision §8.28 (follow-up) — the captain's object-slot cap. A character's cap
 * comes from its own passive (`threeWeaponSlots`, `twoAccessorySlots`); a
 * captain has no such passive and no printed slot line, so it keeps the default
 * of one object per subtype. Rust: `board::CAPTAIN_MAX_OBJECT_SLOTS`.
 */
export const CAPTAIN_MAX_OBJECT_SLOTS = 1;

/**
 * Has the captain a free slot for an object of `objDef`'s subtype?
 * Rust: `board::captain_has_free_object_slot`.
 */
export function captainHasFreeObjectSlot(
  state: GameState,
  playerId: PlayerId,
  objDef: CardDef
): boolean {
  if (!objDef.subtype) return true;
  let same = 0;
  for (const id of state.players[playerId].captain.attachedObjects ?? []) {
    const inst = state.cards[id];
    if (!inst) continue;
    if (getCardDef(inst.defId).subtype === objDef.subtype) same += 1;
  }
  return same < CAPTAIN_MAX_OBJECT_SLOTS;
}

/** Check if character has summoning sickness (deployed this turn, no Rush) */
export function hasSummoningSickness(
  state: GameState,
  instanceId: string
): boolean {
  const card = state.cards[instanceId];
  if (!card) return false;
  if (card.deployedTurn === state.turnNumber) {
    return !hasTrait(state, instanceId, "rush");
  }
  return false;
}

/** Effective deploy cost after costReduction passives (Sengoku) and ship reductions (min 1). */
export function deployCost(
  state: GameState,
  playerId: PlayerId,
  def: import("@/types").CardDef
): number {
  let cost = def.cost;
  const player = state.players[playerId];
  const matches = (f?: { faction?: string; tag?: string; trait?: string }) => {
    if (!f) return true;
    if (f.faction && def.faction !== f.faction) return false;
    if (f.tag && !(def.tags?.includes(f.tag))) return false;
    if (f.trait && !(def.traits?.includes(f.trait as Trait))) return false;
    return true;
  };
  for (const slot of ALL_SLOTS) {
    const id = player.board[slot];
    if (!id) continue;
    const d = getCardDef(state.cards[id].defId);
    for (const e of d.passive?.effects ?? []) {
      if (e.type === "costReduction" && matches(e.filter)) cost -= e.amount;
    }
  }
  if (player.activeShip) {
    const sd = getCardDef(state.cards[player.activeShip].defId);
    const sp = (sd.shipPassive ?? "").toLowerCase();
    if ((sp.includes("cout") || sp.includes("coût")) && sp.includes("-1")) {
      const factionOk =
        (sp.includes("marine") && def.faction === "marine") ||
        (sp.includes("mugiwara") && (def.tags?.includes("mugiwara") ?? false)) ||
        (!sp.includes("marine") && !sp.includes("mugiwara"));
      if (factionOk) cost -= 1;
    }
  }
  return Math.max(1, cost);
}

// ============================================================
// Mutations
// ============================================================

/**
 * Deploy a character from hand to a board slot.
 * Pays the Volonte cost. Marks summoning sickness.
 */
export function deployCharacter(
  state: GameState,
  playerId: PlayerId,
  instanceId: string,
  slot: Slot
): GameState {
  const card = state.cards[instanceId];
  if (!card) throw new Error(`Card not found: ${instanceId}`);
  if (card.owner !== playerId) throw new Error("Not your card");
  if (card.zone !== "hand") throw new Error("Card not in hand");

  const def = getCardDef(card.defId);
  if (def.type !== "character") throw new Error("Not a character card");

  const player = state.players[playerId];
  if (player.board[slot] !== null) throw new Error(`Slot ${slot} is occupied`);

  const cost = deployCost(state, playerId, def);
  if (!canAfford(state, playerId, cost)) {
    throw new Error(`Cannot afford ${def.name} (cost ${cost})`);
  }

  let next = spendVolonte(state, playerId, cost);

  next = produce(next, (draft) => {
    const p = draft.players[playerId];
    const c = draft.cards[instanceId];

    // Remove from hand
    p.hand = p.hand.filter((id) => id !== instanceId);

    // Place on board
    p.board[slot] = instanceId;
    c.zone = "board";
    c.slot = slot;
    c.deployedTurn = draft.turnNumber;
    c.currentPv = def.pv ?? 0;

    // At-deploy ship buffs (Going Merry +1 PV, Navire de Guerre +1 DEF, etc.).
    if (p.activeShip) {
      const sd = getCardDef(draft.cards[p.activeShip].defId);
      const sp = (sd.shipPassive ?? "").toLowerCase();
      if (sp.includes("deploiement") || sp.includes("déploiement")) {
        const factionOk =
          (sp.includes("mugiwara") && (def.tags?.includes("mugiwara") ?? false)) ||
          (sp.includes("marine") && def.faction === "marine") ||
          (!sp.includes("mugiwara") && !sp.includes("marine"));
        if (factionOk) {
          const pv = sp.match(/\+(\d+)\s*pv/);
          const dfb = sp.match(/\+(\d+)\s*def/);
          if (pv) { c.currentPv += parseInt(pv[1]); c.modifiers.push({ id: `shipdep_pv_${instanceId}`, stat: "pv", amount: parseInt(pv[1]), source: `ship_${sd.id}`, duration: "permanent" }); }
          if (dfb) { c.modifiers.push({ id: `shipdep_def_${instanceId}`, stat: "def", amount: parseInt(dfb[1]), source: `ship_${sd.id}`, duration: "permanent" }); }
        }
      }
    }
  });

  next = addLog(next, playerId, `Deploie ${def.name} en ${slot}`);

  // Mr. 2 (Bon Clay): copy the ATK of one of your other characters on deploy.
  if (def.passive?.effects.some((e) => e.type === "copyAtkOnDeploy")) {
    let bestAtk = def.atk ?? 0;
    for (const s of ALL_SLOTS) {
      const oid = next.players[playerId].board[s];
      if (!oid || oid === instanceId) continue;
      bestAtk = Math.max(bestAtk, getEffectiveAtk(next, oid));
    }
    const delta = bestAtk - (def.atk ?? 0);
    if (delta > 0) {
      next = produce(next, (d) => {
        // Decision §8.1/§8.2 (même classe de bug) : le modificateur portait
        // `source: passive_<id>`, et le `recalculatePassiveBuffs` qui clôt
        // `deployCharacter` efface tout modificateur de source `passive_*` — la
        // copie de Mr. 2 ne survivait donc jamais à son propre déploiement.
        // Comme la rage de synergie (§8.1) et la Vantardise (§8.2), elle reçoit
        // une source qui lui est propre ; l'identifiant ne bouge pas.
        d.cards[instanceId].modifiers.push({ id: `manemane_${instanceId}`, stat: "atk", amount: delta, source: `manemane_${instanceId}`, duration: "permanent" });
      });
    }
  }

  // Miss All Sunday (Robin): opponent discards a random card on entry.
  if (def.passive?.effects.some((e) => e.type === "entryDiscardRandom")) {
    const opp = getOpponent(playerId);
    if (next.players[opp].hand.length > 0) {
      next = produce(next, (d) => {
        const h = d.players[opp].hand;
        const i = Math.floor(Math.random() * h.length);
        const [disc] = h.splice(i, 1);
        d.cards[disc].zone = "graveyard";
        d.players[opp].graveyard.push(disc);
        d.log.push({ turn: d.turnNumber, player: playerId, message: `${def.name} : l'adversaire défausse une carte.` });
      });
    }
  }

  // Recalculate passive buffs (new character on board may trigger synergies, captain buffs)
  const { recalculatePassiveBuffs, applyEnemyDebuffAuras } = require("./passives");
  next = recalculatePassiveBuffs(next, playerId);
  next = applyEnemyDebuffAuras(next);

  return next;
}

/**
 * Equip an object from hand onto a character on the board.
 */
export function equipObject(
  state: GameState,
  playerId: PlayerId,
  objectInstanceId: string,
  targetInstanceId: string,
  targetIsCaptain = false
): GameState {
  // Decision §8.28 (follow-up): "Équipable sur Luffy / Crocodile / Akainu"
  // names a **captain**, so the captain is a legal bearer — see
  // `equipObjectOnCaptain` for the rule and its bounds.
  if (targetIsCaptain) {
    return equipObjectOnCaptain(state, playerId, objectInstanceId);
  }
  const objCard = state.cards[objectInstanceId];
  if (!objCard) throw new Error(`Object not found: ${objectInstanceId}`);
  if (objCard.owner !== playerId) throw new Error("Not your card");
  if (objCard.zone !== "hand") throw new Error("Object not in hand");

  const objDef = getCardDef(objCard.defId);
  if (objDef.type !== "object") throw new Error("Not an object card");

  const targetCard = state.cards[targetInstanceId];
  if (!targetCard) throw new Error(`Target not found: ${targetInstanceId}`);
  if (targetCard.owner !== playerId) throw new Error("Not your character");
  if (targetCard.zone !== "board") throw new Error("Target not on board");

  // Decision §8.28: an object is worn by a *character* — never by the active
  // ship, which also lives in zone "board".
  const targetDefEarly = getCardDef(targetCard.defId);
  if (targetDefEarly.type !== "character") throw new Error("Target is not a character");
  // Decision §8.28: "Équipable sur …" is enforced — the bearer must match the
  // printed restriction by name ("Zoro", "Nami", "Mr. 4") or by tag
  // ("bretteur", "tireur").
  if (objDef.restriction && !equipRestrictionOk(targetDefEarly, objDef.restriction)) {
    throw new Error(
      `${targetDefEarly.name} ne peut pas equiper ${objDef.name} (reserve a ${objDef.restriction})`
    );
  }

  // Clima-Tact combo: costs 0 if both Usopp (MG-004) and Nami (MG-003) are in play.
  let effectiveCost = objDef.cost;
  if (objDef.id === "MG-012") {
    const ids = getBoardCharacters(state, playerId).map((c) => c.defId);
    if (ids.includes("MG-003") && ids.includes("MG-004")) effectiveCost = 0;
  }
  if (!canAfford(state, playerId, effectiveCost)) {
    throw new Error(`Cannot afford ${objDef.name} (cost ${effectiveCost})`);
  }

  // Check equipment slot limits (simplified — 1 weapon, 1 fruit, 1 accessory)
  const targetDef = getCardDef(targetCard.defId);
  const existingObjects = targetCard.attachedObjects.map(
    (id) => getCardDef(state.cards[id].defId)
  );

  if (objDef.subtype) {
    const sameSubtype = existingObjects.filter(
      (d) => d.subtype === objDef.subtype
    );
    // Decision §8.28: one cap helper shared with `getValidActions`
    // (Zoro 3 weapons, Franky 2 accessories).
    const maxSlots = maxObjectSlots(targetDef, objDef.subtype);
    if (sameSubtype.length >= maxSlots) {
      throw new Error(
        `${targetDef.name} already has max ${objDef.subtype} equipped`
      );
    }
  }

  let next = spendVolonte(state, playerId, effectiveCost);

  next = produce(next, (draft) => {
    const p = draft.players[playerId];
    const obj = draft.cards[objectInstanceId];
    const target = draft.cards[targetInstanceId];

    // Remove from hand
    p.hand = p.hand.filter((id) => id !== objectInstanceId);

    // Attach to character
    obj.zone = "board";
    obj.slot = target.slot;
    target.attachedObjects.push(objectInstanceId);

    // Signature-weapon bonuses when wielded by the matching character.
    const wielderBonus: Record<string, { name: string; stat: "atk" | "def"; amount: number }> = {
      "MG-009": { name: "Zoro", stat: "def", amount: 1 },        // Wado Ichimonji
      "MR-013": { name: "Tashigi", stat: "atk", amount: 1 },     // Shigure
      "RH-011": { name: "Ben Beckman", stat: "atk", amount: 1 }, // Fusil de Beckman
      "RH-013": { name: "Yasopp", stat: "atk", amount: 1 },      // Fusil de Yasopp
    };
    const wb = wielderBonus[objDef.id];
    if (wb && targetDef.name.includes(wb.name)) {
      target.modifiers.push({ id: `wield_${objectInstanceId}`, stat: wb.stat, amount: wb.amount, source: `equip_${objDef.id}`, duration: "permanent" });
    }
  });

  next = addLog(
    next,
    playerId,
    `Equipe ${objDef.name} sur ${getCardDef(targetCard.defId).name}`
  );

  // Apply Devil Fruit effects if it's a fruit
  if (objDef.subtype === "fruit" && objDef.fruitEffects) {
    const { applyFruitBaseEffects } = require("./fruits");
    next = applyFruitBaseEffects(next, objectInstanceId, targetInstanceId);
  }

  // Recalculate passive buffs
  const { recalculatePassiveBuffs } = require("./passives");
  next = recalculatePassiveBuffs(next, playerId);

  return next;
}

/**
 * Decision §8.28 (follow-up) — equip an object onto the player's own captain.
 *
 * The three signature SR Devil Fruits (`MG-014` Gomu Gomu no Mi, `BW-011` Suna
 * Suna no Mi, `MR-011` Magu Magu no Mi) are printed "Équipable sur Luffy /
 * Crocodile / Akainu"; those names exist in the game only as captains, so under
 * the character-only rule of §8.28 each shipped decklist carried one
 * permanently dead SR card. The captain is the printed bearer and is treated as
 * one here.
 *
 * The rule is deliberately narrow, so nothing outside those printed lines
 * changes: an object reaches the captain **only** through
 * `captainEquipRestrictionOk`, i.e. only when its own printed `restriction`
 * names that captain by name or tag. Everything else is the character path's
 * rule read on the captain: the object must be an object card in the player's
 * hand, the subtype cap is `CAPTAIN_MAX_OBJECT_SLOTS`, the cost is paid, the log
 * line is the same `"Equipe {obj} sur {captain}"`, a fruit runs
 * `applyFruitBaseEffectsOnCaptain` and the passive buffs are recalculated. No
 * face requirement is invented: the captain is in play on both faces.
 *
 * The four signature-weapon wielder bonuses are character-keyed (Zoro /
 * Tashigi / Ben Beckman / Yasopp) and no captain name matches one, so they are
 * deliberately not repeated here.
 *
 * Rust: `board::equip_object_on_captain`.
 */
export function equipObjectOnCaptain(
  state: GameState,
  playerId: PlayerId,
  objectInstanceId: string
): GameState {
  const objCard = state.cards[objectInstanceId];
  if (!objCard) throw new Error(`Object not found: ${objectInstanceId}`);
  if (objCard.owner !== playerId) throw new Error("Not your card");
  if (objCard.zone !== "hand") throw new Error("Object not in hand");

  const objDef = getCardDef(objCard.defId);
  if (objDef.type !== "object") throw new Error("Not an object card");

  const capDef = getCaptainDef(state.players[playerId].captain.defId);
  if (!captainEquipRestrictionOk(capDef, objDef)) {
    throw new Error(
      `${capDef.name} ne peut pas equiper ${objDef.name} (reserve a ${objDef.restriction ?? ""})`
    );
  }

  if (!canAfford(state, playerId, objDef.cost)) {
    throw new Error(`Cannot afford ${objDef.name} (cost ${objDef.cost})`);
  }

  if (!captainHasFreeObjectSlot(state, playerId, objDef)) {
    throw new Error(`${capDef.name} already has max ${objDef.subtype ?? "object"} equipped`);
  }

  let next = spendVolonte(state, playerId, objDef.cost);

  next = produce(next, (draft) => {
    const p = draft.players[playerId];
    const obj = draft.cards[objectInstanceId];

    // Remove from hand
    p.hand = p.hand.filter((id) => id !== objectInstanceId);

    // Attach to the captain. The object stands where its bearer stands —
    // `undefined` while the captain is still recto (off-board), and
    // `flipCaptain` writes the slot when the captain arrives (§8.29).
    obj.zone = "board";
    obj.slot = p.captain.slot;
    p.captain.attachedObjects = [...(p.captain.attachedObjects ?? []), objectInstanceId];
  });

  next = addLog(next, playerId, `Equipe ${objDef.name} sur ${capDef.name}`);

  // Apply Devil Fruit effects if it's a fruit
  if (objDef.subtype === "fruit" && objDef.fruitEffects) {
    const { applyFruitBaseEffectsOnCaptain } = require("./fruits");
    next = applyFruitBaseEffectsOnCaptain(next, objectInstanceId, playerId);
  }

  // Recalculate passive buffs (never strips captain modifiers, so the fruit
  // bonuses live).
  const { recalculatePassiveBuffs } = require("./passives");
  next = recalculatePassiveBuffs(next, playerId);

  return next;
}

/**
 * Decision §8.29 — the equipment stands where its bearer stands: write
 * `targetSlot` onto every attached object. Rust: `board::move_attached_objects`.
 * Called from inside an immer `produce` (the draft is passed in).
 */
export function moveAttachedObjectsInDraft(
  draft: GameState,
  attached: readonly string[],
  targetSlot: Slot
): void {
  for (const objId of attached) {
    const obj = draft.cards[objId];
    if (obj) obj.slot = targetSlot;
  }
}

/**
 * Deploy a ship (max 1 active, replaces previous).
 */
export function deployShip(
  state: GameState,
  playerId: PlayerId,
  instanceId: string
): GameState {
  const card = state.cards[instanceId];
  if (!card) throw new Error(`Card not found: ${instanceId}`);
  if (card.owner !== playerId) throw new Error("Not your card");
  if (card.zone !== "hand") throw new Error("Card not in hand");

  const def = getCardDef(card.defId);
  if (def.type !== "ship") throw new Error("Not a ship card");

  if (!canAfford(state, playerId, def.cost)) {
    throw new Error(`Cannot afford ${def.name} (cost ${def.cost})`);
  }

  let next = spendVolonte(state, playerId, def.cost);

  next = produce(next, (draft) => {
    const p = draft.players[playerId];

    // Discard previous ship if any — may trigger its destruction effect (Going Merry).
    if (p.activeShip) {
      const oldShip = draft.cards[p.activeShip];
      if (oldShip) {
        const oldDef = getCardDef(oldShip.defId);
        const de = oldDef.shipDestroyEffect;
        if (de) {
          if (de.healAll) {
            for (const s of Object.values(p.board)) {
              if (!s) continue;
              const c = draft.cards[s];
              // Decision §8.5: the single heal path — honours `noHeal` /
              // `desiccation` and the permanent max-PV loss, never lowers PV.
              if (c && !c.statusEffects.some((e) => e.type === "noHeal" || e.type === "desiccation")) {
                const printed = getCardDef(c.defId).pv;
                const maxPv = printed === undefined ? c.currentPv : printed - (c.pvMaxLoss ?? 0);
                c.currentPv = Math.max(Math.min(c.currentPv + de.healAll, maxPv), c.currentPv);
              }
            }
          }
          if (de.draw && p.deck.length > 0) {
            for (let i = 0; i < de.draw && p.deck.length > 0; i++) {
              const id = p.deck.shift()!;
              draft.cards[id].zone = "hand";
              p.hand.push(id);
            }
          }
          if (de.deployToken) {
            const empty = ALL_SLOTS.find((s) => p.board[s] === null);
            if (empty) {
              const { generateInstanceId } = require("./utils");
              const tdef = getCardDef(de.deployToken);
              const tid = generateInstanceId(de.deployToken);
              draft.cards[tid] = {
                instanceId: tid, defId: de.deployToken, owner: playerId, zone: "board", slot: empty, tapped: false,
                currentPv: tdef.pv ?? 1, attachedObjects: [], modifiers: [], statusEffects: [],
                deployedTurn: draft.turnNumber, usedBaseAction: false, usedSpecialAttack: false, usedOnceAbilities: [],
              };
              p.board[empty] = tid;
            }
          }
          draft.log.push({ turn: draft.turnNumber, player: playerId, message: `${oldDef.name} : effet de destruction.` });
        }
        oldShip.zone = "graveyard";
        p.graveyard.push(p.activeShip);
      }
    }

    // Remove from hand
    p.hand = p.hand.filter((id) => id !== instanceId);

    // Set as active ship
    p.activeShip = instanceId;
    draft.cards[instanceId].zone = "board";
  });

  next = addLog(next, playerId, `Deploie navire ${def.name}`);
  return next;
}

/**
 * Move a character to an adjacent empty slot (free move, 1x/turn).
 */
export function moveCharacter(
  state: GameState,
  playerId: PlayerId,
  instanceId: string,
  targetSlot: Slot
): GameState {
  const player = state.players[playerId];
  if (player.usedFreeMove) throw new Error("Free move already used this turn");

  const card = state.cards[instanceId];
  if (!card || card.zone !== "board") throw new Error("Card not on board");
  // Decision §8.37 (`betrayal`): a borrowed body is the borrower's to move
  // while the loan lasts — `controllerOf` is `owner` for everything else.
  if (controllerOf(card) !== playerId) throw new Error("Not your card");

  const currentSlot = card.slot;
  if (!currentSlot) throw new Error("Card has no slot");

  const adjacent = getAdjacentSlots(currentSlot);
  if (!adjacent.includes(targetSlot)) {
    throw new Error(`${targetSlot} is not adjacent to ${currentSlot}`);
  }

  if (player.board[targetSlot] !== null) {
    throw new Error(`Slot ${targetSlot} is occupied`);
  }

  return produce(state, (draft) => {
    const p = draft.players[playerId];
    const c = draft.cards[instanceId];

    p.board[currentSlot] = null;
    p.board[targetSlot] = instanceId;
    c.slot = targetSlot;
    p.usedFreeMove = true;
  });
}

/**
 * Remove a character from the board (KO'd).
 * Sends character and attached objects to graveyard.
 */
export function removeFromBoard(
  state: GameState,
  instanceId: string
): GameState {
  const card = state.cards[instanceId];
  if (!card || card.zone !== "board") return state;

  return produce(state, (draft) => {
    const c = draft.cards[instanceId];
    const player = draft.players[c.owner];
    // Decision §8.37 (`betrayal`): a borrowed body sits in the *controller's*
    // board, so that is the cell to clear — `owner` still owns the graveyard.
    const controller = draft.players[controllerOf(c)];
    const slot = c.slot;

    if (slot) {
      controller.board[slot] = null;
    }
    // A body that leaves the board is no longer on loan.
    c.controlledBy = undefined;
    c.loanReturnSlot = undefined;

    // Vivre Card: if the KO'd bearer held one, tutor a Mugiwara (cost <= 3) to hand.
    const hadVivre = c.attachedObjects.some((id) => draft.cards[id]?.defId === "MG-019");
    if (hadVivre) {
      const idx = player.deck.findIndex((id) => {
        const d = getCardDef(draft.cards[id].defId);
        return d.type === "character" && (d.tags?.includes("mugiwara") ?? false) && d.cost <= 3;
      });
      if (idx >= 0) {
        const [tutored] = player.deck.splice(idx, 1);
        draft.cards[tutored].zone = "hand";
        player.hand.push(tutored);
        draft.log.push({ turn: draft.turnNumber, player: c.owner, message: `Vivre Card : ${getCardDef(draft.cards[tutored].defId).name} rejoint la main.` });
      }
    }

    // Move attached objects to graveyard
    for (const objId of c.attachedObjects) {
      const obj = draft.cards[objId];
      if (obj) {
        obj.zone = "graveyard";
        player.graveyard.push(objId);
      }
    }
    c.attachedObjects = [];

    // Move character to graveyard
    c.zone = "graveyard";
    c.slot = undefined;
    player.graveyard.push(instanceId);
  });
}

// ============================================================
// Valid targets for attacks
// ============================================================

/**
 * Get valid attack targets for an attacker.
 * Rules:
 * - Front attacker → any enemy Front
 * - Front attacker with Range → any enemy
 * - Back attacker without Range → cannot melee attack
 * - Back attacker with Range → any enemy
 * - If no enemy Front → can target Back and Captain
 * - Stealth: can't be targeted while non-Stealth ally exists
 * - Captain (verso, on board) → targetable like a normal character
 * - Captain (recto, off board) → targetable if no enemy Front
 */
export function getValidTargets(
  state: GameState,
  attackerInstanceId: string,
  forSpecial?: boolean
): { characterTargets: string[]; canTargetCaptain: boolean } {
  const attacker = state.cards[attackerInstanceId];
  if (!attacker) return { characterTargets: [], canTargetCaptain: false };

  const attackerDef = getCardDef(attacker.defId);
  const attackerSlot = attacker.slot;
  if (!attackerSlot) return { characterTargets: [], canTargetCaptain: false };

  // Decision §8.37 (`betrayal`): a borrowed body fights for whoever controls
  // it this turn, so its legal targets are its *former* allies — reading
  // `owner` here offered it the borrower's own units instead, the inverse of
  // the rule. `controllerOf` is `owner` for every card that is not on loan.
  const opponentId = getOpponent(controllerOf(attacker));
  const opponent = state.players[opponentId];

  // Check range from character trait OR from the specific attack's traits
  let hasRange = attackerDef.traits?.includes("range") ?? false;
  if (!hasRange && forSpecial && attackerDef.specialAttack?.attackTraits?.includes("range")) {
    hasRange = true;
  }
  if (!hasRange && !forSpecial && attackerDef.baseAction?.attackTraits?.includes("range")) {
    hasRange = true;
  }

  const attackerInBack = isBackSlot(attackerSlot);

  // Back row without Range can't melee attack
  if (attackerInBack && !hasRange) {
    return { characterTargets: [], canTargetCaptain: false };
  }

  const opponentHasFront = hasFrontRow(state, opponentId);
  const opponentChars = getBoardCharacters(state, opponentId);

  // Determine targetable characters
  let targetable = opponentChars;

  if (opponentHasFront && !hasRange) {
    // Can only target front row
    targetable = targetable.filter(
      (c) => c.slot && isFrontSlot(c.slot)
    );
  }

  // Apply Stealth filter (a unit stripped of Furtif this turn counts as non-stealth)
  const isStealthed = (c: CardInstance) =>
    hasTrait(state, c.instanceId, "stealth") &&
    !c.statusEffects.some((e) => e.type === "noStealth");
  const hasNonStealth = targetable.some((c) => !isStealthed(c));
  if (hasNonStealth) {
    targetable = targetable.filter((c) => !isStealthed(c));
  }

  // Can target captain?
  let canTargetCaptain = false;
  if (opponent.captain.flipped && opponent.captain.slot) {
    // Verso captain is on board — targetable like a character
    // (subject to front row protection)
    if (!opponentHasFront || hasRange) {
      canTargetCaptain = true;
    } else if (isFrontSlot(opponent.captain.slot)) {
      canTargetCaptain = true;
    }
  } else {
    // Recto captain is off-board, but becomes EXPOSED when its owner has no characters
    // on the board — the crew is wiped (Rulebook v3.1 §2.1). Re-protected as soon as
    // any ally returns to the board.
    canTargetCaptain = opponentChars.length === 0;
  }

  let characterTargets = targetable.map((c) => c.instanceId);

  // Decision §8.38 — Provocation (RH-004) / Peinture de la Colère (BW-005):
  // "Un ennemi doit cibler X a son prochain tour". While the `taunt` status
  // lives on this attacker and the unit that taunted it is still a legal
  // target, that unit is the **only** legal target; a taunter that died, went
  // Furtif or slipped out of range releases the attacker.
  const tauntSource = attacker.statusEffects
    .filter((e) => e.type === "taunt")
    .map((e) => e.source)
    .find((src) => characterTargets.includes(src));
  if (tauntSource !== undefined) {
    characterTargets = [tauntSource];
    canTargetCaptain = false;
  }

  return {
    characterTargets,
    canTargetCaptain,
  };
}

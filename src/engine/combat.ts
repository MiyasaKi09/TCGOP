import { produce } from "immer";
import type {
  GameState,
  PlayerId,
  PendingAttack,
  AttackTrait,
  Element,
  CardInstance,
  SpecialAttack,
} from "@/types";
import { getCardDef, getCaptainDef } from "./cardRegistry";
import {
  getEffectiveAtk,
  getEffectiveDef,
  hasTrait,
  hasSummoningSickness,
  removeFromBoard,
  isFrontSlot,
  getAdjacentSlots,
  getBoardCharacters,
  getValidTargets,
  assertTargetable,
  wornObjectFlag,
  isSlotFree,
  healUnit,
  applyPermanentPvLoss,
  applyCaptainPermanentPvLoss,
  controllerOf,
  grantedAttackTraits,
  attachmentsGrantedAttackTraits,
} from "./board";
import { spendVolonte, canAfford, grantKOBonus } from "./volonte";
import { addLog, getOpponent, checkWinCondition } from "./gameState";
import { defHasNaturalHaki } from "./haki";

// ============================================================
// Step 1: Declare Attack
// ============================================================

/** An attacker's hits can't be dodged if it has the noDodge passive or an equipped Kabuto. */
function attackerNoDodge(state: GameState, attackerInstanceId: string): boolean {
  const card = state.cards[attackerInstanceId];
  if (!card) return false;
  const def = getCardDef(card.defId);
  if (def.passive?.effects.some((e) => e.type === "noDodge")) return true;
  for (const objId of card.attachedObjects) {
    const obj = state.cards[objId];
    if (obj && getCardDef(obj.defId).id === "MG-013") return true; // Kabuto
  }
  // Decision §8.65 — « Si equipee par Lucky Roux : attaques inesquivables ».
  return wornObjectFlag(state, card.attachedObjects, def.name, "noDodge");
}

/**
 * Decision §8.64 — « Les attaques de X ne peuvent etre ni esquivees ni
 * bloquees » (RH-003 Yasopp). Le passif `attacksIgnoreShield` etait declare
 * dans les types et sur la carte, mais AUCUN code ne le lisait : seul
 * `spec.ignoreShield`, porte par une attaque speciale, atteignait
 * `pendingAttack.ignoreShield`. Le pendant `noDodge`, lui, etait bien lu —
 * donc la moitie du texte de Yasopp marchait et l'autre pas.
 *
 * Lu aussi sur les objets portes (RH-010 Gryphon, §8.65).
 */
function attackerIgnoresShield(state: GameState, attackerInstanceId: string): boolean {
  const card = state.cards[attackerInstanceId];
  if (card) {
    const d = getCardDef(card.defId);
    if (d.passive?.effects.some((e) => e.type === "attacksIgnoreShield")) return true;
    return wornObjectFlag(state, card.attachedObjects, d.name, "ignoreShield");
  }
  // L'id synthetique `captain_<joueur>` : le capitaine porte des objets
  // depuis §8.28, et Gryphon (RH-010) nomme Shanks, qui n'existe QUE comme
  // capitaine — sans ce bras la clause serait injouable sur son porteur.
  const pid = getAttackerOwner(state, attackerInstanceId);
  const cap = state.players[pid].captain;
  return wornObjectFlag(state, cap.attachedObjects ?? [], getCaptainDef(cap.defId).name, "ignoreShield");
}

/** Whether the attacker strips Furtif from targets it hits (Smoker). */
function attackerStripsStealth(state: GameState, attackerInstanceId: string): boolean {
  const card = state.cards[attackerInstanceId];
  if (!card) return false;
  return getCardDef(card.defId).passive?.effects.some((e) => e.type === "stripStealthOnAttack") ?? false;
}

/**
 * Conditional ATK bonus from a special when the target matches a trait/faction.
 * Exported since decision §8.34(a): the captain's special attack reads it too
 * (Rust `combat::conditional_atk_bonus`, `pub` for `captain.rs`).
 */
export function conditionalAtkBonus(
  state: GameState,
  cond: { vsTrait?: import("@/types").Trait; vsFaction?: import("@/types").Faction; amount: number } | undefined,
  targetInstanceId: string,
  targetIsCaptain: boolean,
  defenderId: PlayerId
): number {
  if (!cond) return 0;
  if (targetIsCaptain) {
    const capDef = getCaptainDef(state.players[defenderId].captain.defId);
    if (cond.vsFaction && capDef.faction === cond.vsFaction) return cond.amount;
    return 0;
  }
  const tc = state.cards[targetInstanceId];
  if (!tc) return 0;
  const tdef = getCardDef(tc.defId);
  if (cond.vsFaction && tdef.faction === cond.vsFaction) return cond.amount;
  if (cond.vsTrait && hasTrait(state, targetInstanceId, cond.vsTrait)) return cond.amount;
  return 0;
}

/**
 * Decision §8.38/§8.57 — the enemy-facing half of a support special
 * (`taunt` / `immobilize` / `sleep` / `stripStealth`): effects printed against
 * an opponent's unit. Shared by `getValidActions` and `resolveSupportSpecial`
 * so both split the two audiences the same way.
 * Rust: `combat::support_hits_enemy`.
 */
export function supportHitsEnemy(spec: SpecialAttack): boolean {
  return !!(spec.taunt || spec.immobilize || spec.sleep || spec.stripStealth);
}

/**
 * Decision §8.38/§8.57 — the ally-facing half of a support special
 * (`healAmount` / `buffAllyAtk` / `cleanse`): "Un allié gagne +2 ATK et perd
 * gelé/immobilisé" (RH-009 Stimulant). Rust: `combat::support_helps_ally`.
 */
export function supportHelpsAlly(spec: SpecialAttack): boolean {
  return !!((spec.healAmount ?? 0) !== 0 || (spec.buffAllyAtk ?? 0) !== 0 || spec.cleanse);
}

/**
 * Decision §8.38 — enforce the `taunt` status ("Un ennemi doit cibler X à son
 * prochain tour": RH-004 Provocation, BW-005 Peinture de la Colère) at the
 * **executor**, not only in the enumerator.
 *
 * While a `taunt` status lives on the attacker and the instance that taunted it
 * is still a legal target, that unit is the only thing the attacker may declare
 * against — a taunter that died, went Furtif or slipped out of range releases
 * it, which is exactly the binding `getValidTargets` computes. A captain
 * attacker cannot be taunted on the shipped catalogue (`resolveSupportSpecial`,
 * the only writer, targets a card instance), so its binding is the plain "the
 * taunter is still an enemy on the board" test.
 *
 * Rust: `combat::enforce_taunt`. Throws `{name} doit cibler {taunter} (Provocation)`.
 */
export function enforceTaunt(
  state: GameState,
  attackerInstanceId: string,
  targetInstanceId: string,
  targetIsCaptain: boolean,
  forSpecial: boolean
): void {
  const attacker = state.cards[attackerInstanceId];
  let bound: string | undefined;
  if (attacker) {
    if (!attacker.statusEffects.some((e) => e.type === "taunt")) return;
    const legal = getValidTargets(state, attackerInstanceId, forSpecial);
    bound = attacker.statusEffects
      .filter((e) => e.type === "taunt")
      .map((e) => e.source)
      .find((src) => legal.characterTargets.includes(src));
  } else {
    // The synthetic `captain_<player>` attacker id.
    const owner = getAttackerOwner(state, attackerInstanceId);
    const captain = state.players[owner].captain;
    bound = captain.statusEffects
      .filter((e) => e.type === "taunt")
      .map((e) => e.source)
      .find((src) => {
        const c = state.cards[src];
        return !!c && c.zone === "board" && controllerOf(c) !== owner;
      });
  }
  if (bound === undefined) return;
  if (!targetIsCaptain && targetInstanceId === bound) return;

  const attackerName = attacker ? getCardDef(attacker.defId).name : "Le Capitaine";
  const taunterCard = state.cards[bound];
  const taunterName = taunterCard ? getCardDef(taunterCard.defId).name : bound;
  throw new Error(`${attackerName} doit cibler ${taunterName} (Provocation)`);
}

/**
 * Decision §8.61 — garde unique de legalite de cible a la declaration.
 *
 * `getValidTargets` decide ce que l'interface et l'IA PEUVENT proposer, mais
 * aucune des fonctions `declare*` ne recoupait la cible recue : un appel direct
 * au moteur passait outre. On enchaine donc ici les deux liens qui retirent une
 * cible — l'Inciblable (BW-026) puis la Provocation (§8.38).
 *
 * Rust : `combat::enforce_target_legality`.
 */
export function enforceTargetLegality(
  state: GameState,
  attackerInstanceId: string,
  targetInstanceId: string,
  targetIsCaptain: boolean,
  forSpecial: boolean
): void {
  const attacker = state.cards[attackerInstanceId];
  const attackerSide = attacker
    ? controllerOf(attacker)
    : getAttackerOwner(state, attackerInstanceId);
  assertTargetable(state, getOpponent(attackerSide), targetInstanceId, targetIsCaptain);
  enforceTaunt(state, attackerInstanceId, targetInstanceId, targetIsCaptain, forSpecial);
}

/**
 * Decision §8.38 — a *support* special resolves its structured fields and never
 * builds a pending attack: there is nothing for the defender to counter, block
 * or dodge. Rust: `combat::resolve_support_special`.
 */
function resolveSupportSpecial(
  state: GameState,
  owner: PlayerId,
  attackerInstanceId: string,
  defName: string,
  spec: SpecialAttack,
  targetInstanceId: string
): GameState {
  const heal = spec.healAmount ?? 0;
  const buff = spec.buffAllyAtk ?? 0;
  const cleanse = !!spec.cleanse;
  const taunt = !!spec.taunt;
  const immobilize = !!spec.immobilize;
  const sleep = !!spec.sleep;
  const stripStealth = !!spec.stripStealth;

  const needsTarget = heal !== 0 || buff !== 0 || cleanse || taunt || immobilize || sleep || stripStealth;
  const targetCard = state.cards[targetInstanceId];
  const targetOnBoard = !!targetCard && targetCard.zone === "board";
  if (needsTarget && !targetOnBoard) throw new Error("Support special needs a target");

  // Decision §8.38 — the audience split is part of the printed rule, so the
  // executor enforces it and not just the enumerator.
  if (needsTarget) {
    // Decision §8.37 (`betrayal`): a borrowed body counts as an ally of its
    // borrower for the turn, which is what `getValidActions` enumerates too.
    const targetSide = targetCard ? controllerOf(targetCard) : owner;
    // Same precedence as `getValidActions`: a special carrying both audiences
    // (none shipped does) is enumerated against the enemy.
    if (supportHitsEnemy(spec)) {
      if (targetSide === owner) throw new Error("Support special: cet effet vise un ennemi");
    } else if (supportHelpsAlly(spec) && targetSide !== owner) {
      throw new Error("Support special: cet effet vise un allie");
    }
  }

  let next = spendVolonte(state, owner, spec.cost);
  next = produce(next, (draft) => {
    const a = draft.cards[attackerInstanceId];
    a.tapped = true;
    // One action per turn (Rulebook v3.1 §2.2/§6).
    a.usedBaseAction = true;
    a.usedSpecialAttack = true;
    if (spec.oncePerGame) a.usedOnceAbilities.push(spec.name);
  });

  if (!needsTarget) {
    return addLog(next, owner, `${defName} utilise ${spec.name} !`);
  }

  const targetDef = getCardDef(next.cards[targetInstanceId].defId);
  const targetName = targetDef.name;
  const ctrlImmune = targetDef.passive?.effects.some((e) => e.type === "immuneControl") ?? false;

  if (heal !== 0) {
    // Decision §8.5: the single heal path.
    const h = healUnit(next, targetInstanceId, heal);
    next = h.state;
    next = addLog(next, owner, `${defName} utilise ${spec.name} : +${h.healed} PV a ${targetName}`);
  }
  if (buff !== 0) {
    next = produce(next, (draft) => {
      draft.cards[targetInstanceId].modifiers.push({
        id: `support_${spec.name}_${Date.now()}`,
        stat: "atk",
        amount: buff,
        source: `support_${spec.name}`,
        duration: "turn",
      });
    });
    next = addLog(next, owner, `${defName} utilise ${spec.name} : ${targetName} +${buff} ATK ce tour.`);
  }
  if (cleanse) {
    next = produce(next, (draft) => {
      const t = draft.cards[targetInstanceId];
      t.statusEffects = t.statusEffects.filter(
        (e) => e.type !== "freeze" && e.type !== "immobilize" && e.type !== "sleep" && e.type !== "loseAction"
      );
    });
    next = addLog(next, owner, `${defName} utilise ${spec.name} : ${targetName} perd gelé/immobilisé !`);
  }
  if (taunt) {
    next = produce(next, (draft) => {
      draft.cards[targetInstanceId].statusEffects.push({
        type: "taunt",
        // 2 so it survives the start-of-turn decrement and binds the target's *next* turn.
        turnsRemaining: 2,
        damagePerTurn: 0,
        source: attackerInstanceId,
      });
    });
    next = addLog(
      next,
      owner,
      `${defName} utilise ${spec.name} : ${targetName} doit cibler ${defName} a son prochain tour !`
    );
  }
  if (immobilize && !ctrlImmune) {
    next = produce(next, (draft) => {
      draft.cards[targetInstanceId].statusEffects.push({ type: "immobilize", turnsRemaining: 2, damagePerTurn: 0, source: attackerInstanceId });
    });
    next = addLog(next, owner, `${targetName} est immobilisé !`);
  }
  if (sleep && !ctrlImmune) {
    next = produce(next, (draft) => {
      draft.cards[targetInstanceId].statusEffects.push({ type: "sleep", turnsRemaining: 3, damagePerTurn: 0, source: attackerInstanceId });
    });
    next = addLog(next, owner, `${targetName} est endormi !`);
  }
  if (stripStealth) {
    next = produce(next, (draft) => {
      draft.cards[targetInstanceId].statusEffects.push({ type: "noStealth", turnsRemaining: 2, damagePerTurn: 0, source: attackerInstanceId });
    });
  }
  return next;
}

/**
 * Declare a base attack (free, taps the attacker).
 */
export function declareBaseAttack(
  state: GameState,
  attackerInstanceId: string,
  targetInstanceId: string,
  targetIsCaptain: boolean
): GameState {
  const attacker = state.cards[attackerInstanceId];
  if (!attacker) throw new Error("Attacker not found");
  if (attacker.tapped) throw new Error("Attacker is tapped");
  if (attacker.usedBaseAction) throw new Error("Base action already used");
  if (hasSummoningSickness(state, attackerInstanceId)) {
    throw new Error("Character has summoning sickness");
  }

  const def = getCardDef(attacker.defId);
  if (getEffectiveAtk(state, attackerInstanceId) <= 0) {
    throw new Error("Character has 0 ATK — cannot attack");
  }

  // Decision §8.38: a taunted unit may only declare against its taunter.
  // Checked before the trap so a refused declaration cannot eat it (§8.12).
  enforceTargetLegality(state, attackerInstanceId, targetInstanceId, targetIsCaptain, false);

  // Trigger trap on attacker if present
  const trapEffect = attacker.statusEffects.find((e) => e.type === "trap");
  if (trapEffect) {
    state = produce(state, (draft) => {
      const a = draft.cards[attackerInstanceId];
      a.currentPv -= trapEffect.damagePerTurn;
      a.statusEffects = a.statusEffects.filter((e) => e.type !== "trap");
    });
    state = addLog(
      state,
      attacker.owner,
      `Piege ! ${def.name} subit ${trapEffect.damagePerTurn} degats en attaquant !`
    );
    // Check if attacker is KO'd by trap
    if (state.cards[attackerInstanceId].currentPv <= 0) {
      state = addLog(state, attacker.owner, `${def.name} est KO par le piege !`);
      state = removeFromBoard(state, attackerInstanceId);
      return state;
    }
  }

  // Decision §8.37 (`betrayal`): a borrowed body attacks for whoever controls
  // it this turn; `controllerOf` is `owner` for every card that is not on loan.
  const actingOwner = controllerOf(attacker);
  const baseAction = def.baseAction;
  const atk = getEffectiveAtk(state, attackerInstanceId);
  const attackTraits: AttackTrait[] = [...(baseAction?.attackTraits ?? [])];
  // Decision §8.47 : le bras `AttackTrait` du `grantsTraits` d'un objet porté
  // rejoint chaque attaque déclarée par son porteur.
  for (const at of grantedAttackTraits(state, attackerInstanceId)) {
    if (!attackTraits.includes(at)) attackTraits.push(at);
  }

  // Check equipped objects for granted element (e.g. Baril d'Eau grants "water")
  let attackElement = baseAction?.element;
  for (const objId of attacker.attachedObjects) {
    const objCard = state.cards[objId];
    if (objCard) {
      const objDef = getCardDef(objCard.defId);
      if (objDef.grantsElement) {
        attackElement = objDef.grantsElement;
      }
    }
  }

  // Calculate raw damage against target
  let targetDef = 0;
  if (targetIsCaptain) {
    const opponent = getOpponent(actingOwner);
    const cap = state.players[opponent].captain;
    const capDef = getCaptainDef(cap.defId);
    targetDef = cap.flipped ? capDef.verso.def : capDef.recto.def;
    // Apply captain modifiers
    for (const mod of cap.modifiers) {
      if (mod.stat === "def") targetDef += mod.amount;
    }
  } else {
    targetDef = getEffectiveDef(state, targetInstanceId);
  }

  // Apply Piercing (DEF / 2)
  const isPiercing =
    attackTraits.includes("piercing") ||
    (def.traits?.includes("piercing") ?? false);
  if (isPiercing) {
    targetDef = Math.floor(targetDef / 2);
  }

  const rawDamage = Math.max(0, atk - targetDef);

  // Haki to pierce Logia: natural Haki, Armament passive (T7+), or Water element (Rulebook v3.1 §7/§9).
  const hasHaki =
    defHasNaturalHaki(def) ||
    // Decision §8.65 — « Attaques : Haki Armement » (RH-010 Gryphon) : l'objet
    // porte donne le Haki a son porteur, quel que soit le tour.
    wornObjectFlag(state, attacker.attachedObjects, def.name, "grantsHaki") ||
    state.turnNumber >= 7 ||
    attackElement === "water" ||
    !!state.players[actingOwner].hakiThisTurn;

  const pending: PendingAttack = {
    attackerId: attackerInstanceId,
    targetId: targetInstanceId,
    targetIsCaptain,
    isSpecial: false,
    rawDamage,
    attackPower: atk,
    element: attackElement,
    attackTraits,
    hasHaki: hasHaki ?? false,
    cannotBeDodged: attackerNoDodge(state, attackerInstanceId),
    ignoreShield: attackerIgnoresShield(state, attackerInstanceId),
    immobilize: baseAction?.immobilize,
    stripStealth: baseAction?.stripStealth || attackerStripsStealth(state, attackerInstanceId),
  };

  let next = produce(state, (draft) => {
    draft.cards[attackerInstanceId].tapped = true;
    // One action per turn (Rulebook v3.1 §2.2/§6): base OR special, never both.
    draft.cards[attackerInstanceId].usedBaseAction = true;
    draft.cards[attackerInstanceId].usedSpecialAttack = true;
    draft.pendingAttack = pending;
  });

  const targetName = targetIsCaptain
    ? "Capitaine"
    : getCardDef(state.cards[targetInstanceId].defId).name;
  next = addLog(
    next,
    actingOwner,
    `${def.name} attaque ${targetName} (ATK ${atk} vs DEF ${targetDef} = ${rawDamage} degats)`
  );

  return next;
}

/**
 * Declare a special attack (costs Vol., does NOT tap — can combo with base).
 */
export function declareSpecialAttack(
  state: GameState,
  attackerInstanceId: string,
  targetInstanceId: string,
  targetIsCaptain: boolean
): GameState {
  const attacker = state.cards[attackerInstanceId];
  if (!attacker) throw new Error("Attacker not found");
  if (attacker.usedSpecialAttack) throw new Error("Special already used");
  if (hasSummoningSickness(state, attackerInstanceId)) {
    throw new Error("Character has summoning sickness");
  }

  const def = getCardDef(attacker.defId);

  // Decision §8.38: a taunted unit may only declare against its taunter. A
  // self-transformation targets itself and an ally-facing support special is
  // not an attack at all, so neither is bound; an enemy-facing support special
  // is. Checked before the trap so a refused declaration cannot eat it (§8.12).
  if (
    def.specialAttack &&
    !def.specialAttack.transform &&
    (!def.specialAttack.isSupport || supportHitsEnemy(def.specialAttack))
  ) {
    enforceTargetLegality(state, attackerInstanceId, targetInstanceId, targetIsCaptain, true);
  }

  // Trigger trap on attacker if present
  const trapEffectSpec = attacker.statusEffects.find((e) => e.type === "trap");
  if (trapEffectSpec) {
    state = produce(state, (draft) => {
      const a = draft.cards[attackerInstanceId];
      a.currentPv -= trapEffectSpec.damagePerTurn;
      a.statusEffects = a.statusEffects.filter((e) => e.type !== "trap");
    });
    state = addLog(
      state,
      attacker.owner,
      `Piege ! ${def.name} subit ${trapEffectSpec.damagePerTurn} degats en attaquant !`
    );
    if (state.cards[attackerInstanceId].currentPv <= 0) {
      state = addLog(state, attacker.owner, `${def.name} est KO par le piege !`);
      state = removeFromBoard(state, attackerInstanceId);
      return state;
    }
  }

  const spec = def.specialAttack;
  if (!spec) throw new Error("Character has no special attack");

  // Decision §8.37 (`betrayal`): a borrowed body attacks — and pays — for
  // whoever controls it this turn; `controllerOf` is `owner` off loan.
  const actingOwner = controllerOf(attacker);

  // Check 1x/game
  if (spec.oncePerGame && attacker.usedOnceAbilities.includes(spec.name)) {
    throw new Error("Already used this ability (1x/game)");
  }

  if (!canAfford(state, actingOwner, spec.cost)) {
    throw new Error(`Cannot afford special (cost ${spec.cost})`);
  }

  // Self-transformation special (Chopper Monster Point): no target, no pending attack.
  if (spec.transform) {
    const t = spec.transform;
    let tnext = spendVolonte(state, actingOwner, spec.cost);
    const curAtk = getEffectiveAtk(tnext, attackerInstanceId);
    tnext = produce(tnext, (draft) => {
      const c = draft.cards[attackerInstanceId];
      c.usedBaseAction = true;
      c.usedSpecialAttack = true;
      c.tapped = true;
      if (spec.oncePerGame) c.usedOnceAbilities.push(spec.name);
      c.modifiers.push({
        id: `transform_${spec.name}_${Date.now()}`,
        stat: "atk", amount: Math.max(0, t.atk - curAtk),
        source: "transform", duration: "permanent",
      });
      // Self-KO countdown — KO'd after `turns` of the owner's turns (no Vol to opponent).
      c.statusEffects.push({ type: "selfKO", turnsRemaining: t.turns, damagePerTurn: 0, source: spec.name });
    });
    return addLog(tnext, actingOwner, `${def.name} : ${spec.name} ! ATK ${t.atk} pendant ${t.turns} tours, puis KO.`);
  }

  // Decision §8.38: a *support* special resolves its structured fields and never
  // builds a pending attack — there is nothing to counter, block or dodge.
  if (spec.isSupport) {
    return resolveSupportSpecial(state, actingOwner, attackerInstanceId, def.name, spec, targetInstanceId);
  }

  let next = spendVolonte(state, actingOwner, spec.cost);

  const baseAtk = getEffectiveAtk(state, attackerInstanceId);
  const condBonus = conditionalAtkBonus(state, spec.conditionalBonus, targetInstanceId, targetIsCaptain, getOpponent(actingOwner));
  const totalAtk = baseAtk + spec.atkBonus + condBonus;

  // "Touche 2 cibles" is approximated as a small Zone (target + adjacents).
  const attackTraits: AttackTrait[] = [...(spec.attackTraits ?? []), ...(spec.twoTargets && !(spec.attackTraits ?? []).includes("zone") ? ["zone" as AttackTrait] : [])];
  // Decision §8.47 : même fusion que sur l'attaque de base.
  for (const at of grantedAttackTraits(state, attackerInstanceId)) {
    if (!attackTraits.includes(at)) attackTraits.push(at);
  }

  let targetDefVal = 0;
  if (targetIsCaptain) {
    const opponent = getOpponent(actingOwner);
    const cap = next.players[opponent].captain;
    const capDef = getCaptainDef(cap.defId);
    targetDefVal = cap.flipped ? capDef.verso.def : capDef.recto.def;
    for (const mod of cap.modifiers) {
      if (mod.stat === "def") targetDefVal += mod.amount;
    }
  } else {
    targetDefVal = getEffectiveDef(next, targetInstanceId);
  }

  // Piercing
  const isPiercing =
    attackTraits.includes("piercing") ||
    (def.traits?.includes("piercing") ?? false);
  if (isPiercing) {
    targetDefVal = Math.floor(targetDefVal / 2);
  }

  // Ignore DEF
  if (spec.ignoreDef) {
    targetDefVal = Math.max(0, targetDefVal - spec.ignoreDef);
  }

  const rawDamage = Math.max(0, totalAtk - targetDefVal);

  // Haki to pierce Logia: natural Haki, Armament passive (T7+), or Water element (Rulebook v3.1 §7/§9).
  const hasHaki =
    defHasNaturalHaki(def) ||
    // Decision §8.65 — « Attaques : Haki Armement » (RH-010 Gryphon) : l'objet
    // porte donne le Haki a son porteur, quel que soit le tour.
    wornObjectFlag(state, attacker.attachedObjects, def.name, "grantsHaki") ||
    state.turnNumber >= 7 ||
    spec.element === "water" ||
    !!state.players[actingOwner].hakiThisTurn;

  const pending: PendingAttack = {
    attackerId: attackerInstanceId,
    targetId: targetInstanceId,
    targetIsCaptain,
    isSpecial: true,
    rawDamage,
    attackPower: totalAtk,
    element: spec.element,
    attackTraits,
    hasHaki: hasHaki ?? false,
    cannotBeDodged: spec.cannotBeDodged || attackerNoDodge(state, attackerInstanceId),
    ignoreShield: spec.ignoreShield || attackerIgnoresShield(state, attackerInstanceId),
    immobilize: spec.immobilize,
    sleep: spec.sleep,
    pushback: spec.pushback || (spec.pushbackSlots ?? 0) > 0,
    stripStealth: spec.stripStealth || attackerStripsStealth(state, attackerInstanceId),
    // Decision §8.38: "La cible perd N PV permanent (Sable)" / "… et ne peut
    // plus etre soignee" ride on the pending attack and resolve after damage.
    permanentPvLoss: spec.permanentPvLoss,
    noHeal: spec.noHeal,
  };

  next = produce(next, (draft) => {
    draft.cards[attackerInstanceId].tapped = true;
    // One action per turn (Rulebook v3.1 §2.2/§6): base OR special, never both.
    draft.cards[attackerInstanceId].usedSpecialAttack = true;
    draft.cards[attackerInstanceId].usedBaseAction = true;
    if (spec.oncePerGame) {
      draft.cards[attackerInstanceId].usedOnceAbilities.push(spec.name);
    }
    draft.pendingAttack = pending;
  });

  const targetName = targetIsCaptain
    ? "Capitaine"
    : getCardDef(next.cards[targetInstanceId].defId).name;
  next = addLog(
    next,
    actingOwner,
    `${def.name} utilise ${spec.name} sur ${targetName} (ATK ${totalAtk} vs DEF ${targetDefVal} = ${rawDamage} degats)`
  );

  return next;
}

/**
 * Declare a fruit awakening special attack.
 */
export function declareFruitSpecialAttack(
  state: GameState,
  attackerInstanceId: string,
  fruitInstanceId: string,
  targetInstanceId: string,
  targetIsCaptain: boolean
): GameState {
  // Decision §8.28 (follow-up): the three signature SR fruits are worn by a
  // captain, so the awakened special can be declared *by* a captain — addressed
  // by the same synthetic `captain_{playerId}` id every other captain target
  // and attacker uses. Rust: `combat::declare_fruit_special_attack`.
  if (attackerInstanceId.startsWith("captain_")) {
    return declareCaptainFruitSpecialAttack(
      state,
      attackerInstanceId.replace("captain_", "") as PlayerId,
      fruitInstanceId,
      targetInstanceId,
      targetIsCaptain
    );
  }
  const attacker = state.cards[attackerInstanceId];
  if (!attacker) throw new Error("Attacker not found");
  if (hasSummoningSickness(state, attackerInstanceId)) {
    throw new Error("Character has summoning sickness");
  }

  // Decision §8.38: a taunted unit may only declare against its taunter.
  enforceTargetLegality(state, attackerInstanceId, targetInstanceId, targetIsCaptain, true);

  const fruitCard = state.cards[fruitInstanceId];
  if (!fruitCard || !fruitCard.isAwakened) throw new Error("Fruit not awakened");

  const fruitDef = getCardDef(fruitCard.defId);
  const spec = fruitDef.fruitEffects?.awakening?.specialAttack;
  if (!spec) throw new Error("Fruit has no awakening special attack");

  // Decision §8.37 (`betrayal`): the awakened-fruit special is a declaration
  // like its two siblings — it acts for the body's controller.
  const actingOwner = controllerOf(attacker);

  // Check once per game
  if (spec.oncePerGame && attacker.usedOnceAbilities.includes(spec.name)) {
    throw new Error("Already used this fruit ability (1x/game)");
  }

  if (!canAfford(state, actingOwner, spec.cost)) {
    throw new Error(`Cannot afford fruit special (cost ${spec.cost})`);
  }

  let next: GameState = spendVolonte(state, actingOwner, spec.cost);

  const def = getCardDef(attacker.defId);
  const baseAtk = getEffectiveAtk(next, attackerInstanceId);
  const totalAtk = baseAtk + spec.atkBonus;

  const attackTraits: AttackTrait[] = [...(spec.attackTraits ?? [])];
  // Decision §8.47 (suivi) : « chaque déclaration » veut dire les trois — la
  // spéciale de fruit éveillé fusionne les mêmes mots-clés que ses deux sœurs.
  for (const at of grantedAttackTraits(state, attackerInstanceId)) {
    if (!attackTraits.includes(at)) attackTraits.push(at);
  }

  let targetDefVal = 0;
  if (targetIsCaptain) {
    const opponent = getOpponent(actingOwner);
    const cap = next.players[opponent].captain;
    const capDef = getCaptainDef(cap.defId);
    targetDefVal = cap.flipped ? capDef.verso.def : capDef.recto.def;
    for (const mod of cap.modifiers) {
      if (mod.stat === "def") targetDefVal += mod.amount;
    }
  } else {
    targetDefVal = getEffectiveDef(next, targetInstanceId);
  }
  if (attackTraits.includes("piercing")) targetDefVal = Math.floor(targetDefVal / 2);
  if (spec.ignoreDef) targetDefVal = Math.max(0, targetDefVal - spec.ignoreDef);

  const rawDamage = Math.max(0, totalAtk - targetDefVal);

  const hasHaki =
    defHasNaturalHaki(def) ||
    // Decision §8.65 — « Attaques : Haki Armement » (RH-010 Gryphon) : l'objet
    // porte donne le Haki a son porteur, quel que soit le tour.
    wornObjectFlag(state, attacker.attachedObjects, def.name, "grantsHaki") || next.turnNumber >= 7 || spec.element === "water";

  const pending: PendingAttack = {
    attackerId: attackerInstanceId,
    targetId: targetInstanceId,
    targetIsCaptain,
    isSpecial: true,
    rawDamage,
    attackPower: totalAtk,
    element: spec.element,
    attackTraits,
    hasHaki: hasHaki ?? false,
    ignoreShield: spec.ignoreShield || attackerIgnoresShield(state, attackerInstanceId),
    immobilize: spec.immobilize,
    sleep: spec.sleep,
    pushback: spec.pushback,
    stripStealth: spec.stripStealth,
    // Decision §8.38 × §8.48: BW-011 "Ground Death" prints both clauses.
    permanentPvLoss: spec.permanentPvLoss,
    noHeal: spec.noHeal,
  };

  next = produce(next, (draft) => {
    draft.cards[attackerInstanceId].tapped = true;
    // One action per turn (Rulebook v3.1 §2.2/§6): base OR special, never both.
    draft.cards[attackerInstanceId].usedSpecialAttack = true;
    draft.cards[attackerInstanceId].usedBaseAction = true;
    if (spec.oncePerGame) {
      draft.cards[attackerInstanceId].usedOnceAbilities.push(spec.name);
    }
    draft.pendingAttack = pending;
  });

  const targetName = targetIsCaptain
    ? "Capitaine"
    : getCardDef(next.cards[targetInstanceId].defId).name;
  next = addLog(
    next,
    actingOwner,
    `${def.name} utilise ${spec.name} sur ${targetName} (ATK ${totalAtk} vs DEF ${targetDefVal} = ${rawDamage} degats)`
  );

  return next;
}

/**
 * Decision §8.28 (follow-up) — the awakened-fruit special declared by a
 * **captain**, the printed bearer of `MG-014` / `BW-011` / `MR-011`.
 *
 * It is a captain attack that happens to be driven by a fruit's awakening
 * `specialAttack`, so it follows `declareCaptainBaseAttack` wherever the two
 * could differ — the verso stat plus the captain's ATK modifiers (the fruit's
 * own `fruit_atk_*` / `fruit_awaken_atk_*` modifiers live there, so they are
 * already in), the captain's flip / tap / one-action-per-turn / frozen /
 * summoning-sickness gates, the captain's `usedOnceAbilities` for
 * `oncePerGame`, the player-wide `hakiThisTurn` and the
 * `"Capitaine {name} utilise …"` log line.
 *
 * Everything the *fruit* contributes is read exactly as
 * `declareFruitSpecialAttack` reads it: `atkBonus`, `element`, `attackTraits`,
 * `ignoreDef`, `ignoreShield`, `immobilize`, `sleep`, `pushback`,
 * `stripStealth` and the §8.38 × §8.48 pair `permanentPvLoss` / `noHeal`
 * (`BW-011`'s Ground Death).
 * Rust: `combat::declare_captain_fruit_special_attack_inner`.
 */
export function declareCaptainFruitSpecialAttack(
  state: GameState,
  playerId: PlayerId,
  fruitInstanceId: string,
  targetInstanceId: string,
  targetIsCaptain: boolean
): GameState {
  const { captainCannotAct, captainHasTraitNow } = require("./captain");
  const captain = state.players[playerId].captain;
  if (!captain.flipped) throw new Error("Captain not flipped (verso required)");
  if (captain.tapped) throw new Error("Captain is tapped");
  if (captain.usedSpecialAttack) throw new Error("Captain special already used");
  if (captainCannotAct(captain)) {
    throw new Error("Captain cannot act (frozen, immobilized or asleep)");
  }
  if (!(captain.attachedObjects ?? []).includes(fruitInstanceId)) {
    throw new Error("Captain is not wearing this fruit");
  }

  const capDef = getCaptainDef(captain.defId);
  if (
    captain.deployedTurn === state.turnNumber &&
    !captainHasTraitNow(state, playerId, "rush")
  ) {
    throw new Error("Captain has summoning sickness");
  }

  const fruitCard = state.cards[fruitInstanceId];
  if (!fruitCard || !fruitCard.isAwakened) throw new Error("Fruit not awakened");
  const fruitDef = getCardDef(fruitCard.defId);
  const spec = fruitDef.fruitEffects?.awakening?.specialAttack;
  if (!spec) throw new Error("Fruit has no awakening special attack");

  if (spec.oncePerGame && captain.usedOnceAbilities.includes(spec.name)) {
    throw new Error("Already used this fruit ability (1x/game)");
  }
  if (!canAfford(state, playerId, spec.cost)) {
    throw new Error(`Cannot afford fruit special (cost ${spec.cost})`);
  }

  // Decision §8.38: every declaration path is bound by a taunt (inert while
  // nothing in the catalogue can taunt a captain).
  enforceTargetLegality(state, `captain_${playerId}`, targetInstanceId, targetIsCaptain, true);

  let next: GameState = spendVolonte(state, playerId, spec.cost);

  let baseAtk = capDef.verso.atk;
  for (const mod of captain.modifiers) {
    if (mod.stat === "atk") baseAtk += mod.amount;
  }
  const totalAtk = baseAtk + spec.atkBonus;

  const attackTraits: AttackTrait[] = [...(spec.attackTraits ?? [])];
  // Decision §8.47 × §8.28 : le capitaine porteur lit le meme bras
  // `AttackTrait` de `grantsTraits` que ses equivalents personnages.
  for (const at of attachmentsGrantedAttackTraits(next, captain.attachedObjects ?? [])) {
    if (!attackTraits.includes(at)) attackTraits.push(at);
  }

  let targetDefVal = 0;
  if (targetIsCaptain) {
    const opponent = getOpponent(playerId);
    const cap = next.players[opponent].captain;
    const oppCapDef = getCaptainDef(cap.defId);
    targetDefVal = cap.flipped ? oppCapDef.verso.def : oppCapDef.recto.def;
    for (const mod of cap.modifiers) {
      if (mod.stat === "def") targetDefVal += mod.amount;
    }
  } else {
    targetDefVal = getEffectiveDef(next, targetInstanceId);
  }
  // Decision §8.55: DEF is clamped at 0 before the halving.
  if (attackTraits.includes("piercing") || captainHasTraitNow(next, playerId, "piercing")) {
    targetDefVal = Math.floor(Math.max(0, targetDefVal) / 2);
  }
  if (spec.ignoreDef) targetDefVal = Math.max(0, targetDefVal - spec.ignoreDef);

  const rawDamage = Math.max(0, totalAtk - targetDefVal);

  const hasHaki =
    ((capDef.verso.naturalHaki && capDef.verso.naturalHaki.length > 0) ?? false) ||
    next.turnNumber >= 7 ||
    spec.element === "water" ||
    !!next.players[playerId].hakiThisTurn;

  const pending: PendingAttack = {
    attackerId: `captain_${playerId}`,
    targetId: targetInstanceId,
    targetIsCaptain,
    isSpecial: true,
    rawDamage,
    attackPower: totalAtk,
    element: spec.element,
    attackTraits,
    hasHaki,
    ignoreShield: spec.ignoreShield || attackerIgnoresShield(state, `captain_${playerId}`),
    immobilize: spec.immobilize,
    sleep: spec.sleep,
    pushback: spec.pushback,
    stripStealth: spec.stripStealth,
    // Decision §8.38 × §8.48: BW-011 "Ground Death" prints both clauses.
    permanentPvLoss: spec.permanentPvLoss,
    noHeal: spec.noHeal,
  };

  next = produce(next, (draft) => {
    const cap = draft.players[playerId].captain;
    cap.tapped = true;
    // One action per turn (Rulebook v3.1 §2.2/§6): base OR special, never both.
    cap.usedSpecialAttack = true;
    cap.usedBaseAction = true;
    if (spec.oncePerGame) cap.usedOnceAbilities.push(spec.name);
    draft.pendingAttack = pending;
  });

  const targetName = targetIsCaptain
    ? "Capitaine"
    : getCardDef(next.cards[targetInstanceId].defId).name;
  next = addLog(
    next,
    playerId,
    `Capitaine ${capDef.name} utilise ${spec.name} sur ${targetName} (ATK ${totalAtk} vs DEF ${targetDefVal} = ${rawDamage} degats)`
  );

  return next;
}

// ============================================================
// Step 2: Counter Window — handled by UI/AI (pass or play counter)
// ============================================================

/**
 * Apply a counter card that reduces damage.
 */
export function applyCounterReduce(
  state: GameState,
  counterInstanceId: string
): GameState {
  if (!state.pendingAttack) throw new Error("No pending attack");

  const counter = state.cards[counterInstanceId];
  if (!counter) throw new Error("Counter not found");
  const counterDef = getCardDef(counter.defId);
  if (!counterDef.counterEffect) throw new Error("Not a counter card");
  if (counterDef.counterEffect.type !== "reduceDamage") {
    throw new Error("Not a damage reduction counter");
  }

  const owner = counter.owner;
  if (!canAfford(state, owner, counterDef.cost)) {
    throw new Error("Cannot afford counter");
  }

  let reduction = counterDef.counterEffect.amount;
  // Captain bonus: if targeting captain, extra reduction
  if (state.pendingAttack.targetIsCaptain && counterDef.counterEffect.captainBonus) {
    reduction = counterDef.counterEffect.captainBonus;
  }

  let next = spendVolonte(state, owner, counterDef.cost);

  next = produce(next, (draft) => {
    const p = draft.players[owner];
    // Remove counter from hand
    p.hand = p.hand.filter((id) => id !== counterInstanceId);
    // Send to graveyard
    draft.cards[counterInstanceId].zone = "graveyard";
    p.graveyard.push(counterInstanceId);
    // Reduce pending damage
    if (draft.pendingAttack) {
      draft.pendingAttack.rawDamage = Math.max(
        0,
        draft.pendingAttack.rawDamage - reduction
      );
    }
  });

  next = addLog(
    next,
    owner,
    `Joue ${counterDef.name} : reduit les degats de ${reduction}`
  );
  return next;
}

/**
 * Cancel an incoming attack (Manteau de Justice, « Faible », Mirage, Sacrifice du Bras).
 */
export function applyCounterCancel(state: GameState, counterInstanceId: string): GameState {
  if (!state.pendingAttack) throw new Error("No pending attack");
  const counter = state.cards[counterInstanceId];
  if (!counter) throw new Error("Counter not found");
  const cdef = getCardDef(counter.defId);
  const ce = cdef.counterEffect;
  if (!ce || (ce.type !== "cancel" && ce.type !== "untargetable")) throw new Error("Not a cancel counter");
  const owner = counter.owner;
  if (!canAfford(state, owner, cdef.cost)) throw new Error("Cannot afford counter");
  if (ce.type === "cancel" && ce.maxAttackerAtk !== undefined) {
    if ((state.pendingAttack.attackPower ?? 0) > ce.maxAttackerAtk) {
      throw new Error("Attacker is too strong for this counter");
    }
  }

  // Lu AVANT que le `produce` ne vide `pendingAttack` : c'est la cible de
  // l'attaque en cours qui devient Inciblable (decision §8.61).
  const protectedTarget = ce.type === "untargetable"
    ? { id: state.pendingAttack.targetId, isCaptain: state.pendingAttack.targetIsCaptain }
    : null;

  let next = spendVolonte(state, owner, cdef.cost);
  next = produce(next, (draft) => {
    const p = draft.players[owner];
    p.hand = p.hand.filter((id) => id !== counterInstanceId);
    draft.cards[counterInstanceId].zone = "graveyard";
    p.graveyard.push(counterInstanceId);
    draft.pendingAttack = null;

    // Decision §8.61 — jusqu'ici le bras `untargetable` ne se distinguait en
    // rien d'un `cancel` : l'attaque tombait et la cible ne gardait aucune
    // trace, donc la deuxieme attaque du meme tour la touchait. On pose
    // maintenant un vrai statut, purge au debut du tour suivant.
    if (protectedTarget) {
      const holder = protectedTarget.isCaptain
        ? draft.players[owner].captain
        : draft.cards[protectedTarget.id];
      if (holder && !holder.statusEffects.some((e) => e.type === "untargetable")) {
        holder.statusEffects.push({
          type: "untargetable",
          turnsRemaining: 1,
          damagePerTurn: 0,
          source: counterInstanceId,
        });
      }
    }
  });
  next = addLog(
    next,
    owner,
    protectedTarget
      ? `${cdef.name} : attaque annulée — la cible est Inciblable jusqu'à la fin du tour.`
      : `${cdef.name} : attaque annulée !`
  );

  if (ce.type === "cancel" && ce.selfCaptainDamage) {
    next = produce(next, (draft) => { draft.players[owner].captain.currentPv -= ce.selfCaptainDamage!; });
    next = addLog(next, owner, `${cdef.name} : votre Capitaine subit ${ce.selfCaptainDamage} dégâts.`);
    const w = checkWinCondition(next);
    if (w) next = produce(next, (draft) => { draft.winner = w; });
  }
  return next;
}

/**
 * Apply a "survive" counter (ally survives at 1 PV).
 * This is resolved at damage application time — mark the counter as played.
 */
export function applyCounterSurvive(
  state: GameState,
  counterInstanceId: string
): GameState {
  if (!state.pendingAttack) throw new Error("No pending attack");

  const counter = state.cards[counterInstanceId];
  if (!counter) throw new Error("Counter not found");
  const counterDef = getCardDef(counter.defId);
  if (counterDef.counterEffect?.type !== "survive") {
    throw new Error("Not a survive counter");
  }

  const owner = counter.owner;
  if (!canAfford(state, owner, counterDef.cost)) {
    throw new Error("Cannot afford counter");
  }

  // Survive protects an ally character — once per character (Rulebook ST01 MG-026).
  const protectedId = state.pendingAttack.targetId;
  if (state.pendingAttack.targetIsCaptain) throw new Error("Survive only protects allies");
  const protectedCard = state.cards[protectedId];
  if (protectedCard?.usedOnceAbilities.includes("survived")) {
    throw new Error("This character already survived once");
  }

  let next = spendVolonte(state, owner, counterDef.cost);

  next = produce(next, (draft) => {
    const p = draft.players[owner];
    p.hand = p.hand.filter((id) => id !== counterInstanceId);
    draft.cards[counterInstanceId].zone = "graveyard";
    p.graveyard.push(counterInstanceId);
    // Mark the pending attack — target survives at 1 PV — and tag the character.
    if (draft.pendingAttack) {
      (draft.pendingAttack as PendingAttack & { survivePlayed?: boolean }).survivePlayed = true;
    }
    const pc = draft.cards[protectedId];
    if (pc) pc.usedOnceAbilities.push("survived");
  });

  next = addLog(next, owner, `Joue ${counterDef.name} : survie a 1 PV !`);
  return next;
}

/**
 * Bouclier / Shield reaction (Rulebook v3.1 §8): an untapped ally with the Shield trait,
 * adjacent to the attack's target, intercepts the hit. It taps, becomes the new target,
 * and the damage is recomputed against its DEF. Costs 0 Vol. The attack stays pending so
 * the defender can still react further or pass to resolve.
 */
export function applyShieldBlock(
  state: GameState,
  blockerInstanceId: string
): GameState {
  const pending = state.pendingAttack;
  if (!pending) throw new Error("No pending attack");
  if (pending.ignoreShield) throw new Error("This attack ignores Bouclier");

  const blocker = state.cards[blockerInstanceId];
  if (!blocker) throw new Error("Blocker not found");
  if (blocker.tapped) throw new Error("A tapped character cannot use Bouclier");
  if (!hasTrait(state, blockerInstanceId, "shield")) {
    throw new Error("Not a Bouclier character");
  }

  let blockerDef = getEffectiveDef(state, blockerInstanceId);
  if (pending.attackTraits.includes("piercing")) {
    blockerDef = Math.floor(blockerDef / 2);
  }
  // Some blockers reduce the damage further (Sentomaru, Garp).
  const blockDef = getCardDef(blocker.defId);
  const blockRed = blockDef.passive?.effects.reduce((s, e) => s + (e.type === "blockDamageReduction" ? e.amount : 0), 0) ?? 0;
  const atkPower = pending.attackPower ?? pending.rawDamage;
  const newRaw = Math.max(0, atkPower - blockerDef - blockRed);

  let next = produce(state, (draft) => {
    draft.cards[blockerInstanceId].tapped = true;
    if (draft.pendingAttack) {
      draft.pendingAttack.targetId = blockerInstanceId;
      draft.pendingAttack.targetIsCaptain = false;
      draft.pendingAttack.rawDamage = newRaw;
    }
  });

  const blockerName = getCardDef(blocker.defId).name;
  return addLog(next, blocker.owner, `🛡 ${blockerName} bloque l'attaque ! (Bouclier)`);
}

// ============================================================
// Step 3: Resolve Attack (apply damage)
// ============================================================

/**
 * Resolve the pending attack — apply damage to target.
 */
export function resolveAttack(state: GameState): GameState {
  const pending = state.pendingAttack as PendingAttack & { survivePlayed?: boolean } | null;
  if (!pending) throw new Error("No pending attack");

  let next = state;

  if (pending.targetIsCaptain) {
    next = applyCaptainDamage(next, pending);
  } else {
    next = applyCharacterDamage(next, pending);
  }

  // Thorns / melee recoil (Miss Doublefinger): a melee attacker takes damage.
  if (!pending.targetIsCaptain && !pending.attackTraits.includes("range")) {
    const tdefPre = getCardDef(state.cards[pending.targetId].defId);
    const recoil = tdefPre.passive?.effects.reduce((s, e) => s + (e.type === "meleeRecoil" ? e.amount : 0), 0) ?? 0;
    const atkId = pending.attackerId;
    if (recoil > 0 && !atkId.startsWith("captain_") && next.cards[atkId]?.zone === "board") {
      const atkDef = getCardDef(next.cards[atkId].defId);
      if (!(atkDef.traits?.includes("range"))) {
        next = produce(next, (d) => { d.cards[atkId].currentPv -= recoil; });
        next = addLog(next, next.cards[atkId].owner, `${atkDef.name} subit ${recoil} dégâts (Épines) !`);
        if (next.cards[atkId].currentPv <= 0) {
          const koDefId = next.cards[atkId].defId; const koOwner = next.cards[atkId].owner;
          next = addLog(next, koOwner, `${atkDef.name} est KO (Épines) !`);
          next = grantKOBonus(next, koOwner);
          next = removeFromBoard(next, atkId);
          const { applyOnKOEffects } = require("./passives");
          next = applyOnKOEffects(next, koOwner, getOpponent(koOwner), koDefId);
        }
      }
    }
  }

  // Apply element effects
  next = applyElementEffects(next, pending);

  // Decision §8.38 — `permanentPvLoss`: "La cible perd N PV permanent (Sable)"
  // (Crocodile's Desert Girasol, BW-011's Ground Death). The maximum drops for
  // good and the current PV follows it down, so a later heal can never climb
  // back over the loss.
  if ((pending.permanentPvLoss ?? 0) > 0) {
    if (pending.targetIsCaptain) {
      const attackerOwner = getAttackerOwner(next, pending.attackerId);
      next = applyCaptainPermanentPvLoss(next, getOpponent(attackerOwner), pending.permanentPvLoss!);
    } else {
      next = applyPermanentPvLoss(next, pending.targetId, pending.permanentPvLoss!);
    }
  }

  // Check KO from element effects (thunder propagation, sand, water x2)
  if (!pending.targetIsCaptain) {
    const targetAfter = next.cards[pending.targetId];
    if (targetAfter && targetAfter.zone === "board" && targetAfter.currentPv <= 0) {
      const targetDefAfter = getCardDef(targetAfter.defId);
      const atkOwner = getAttackerOwner(next, pending.attackerId);
      next = addLog(next, targetAfter.owner, `${targetDefAfter.name} est KO (effet elementaire) !`);
      // +2 Vol. goes to the player who LOST the ally (Rulebook v3.1 §4).
      next = grantKOBonus(next, targetAfter.owner);
      const koDefId = targetAfter.defId;
      const koOwner = targetAfter.owner;
      next = removeFromBoard(next, pending.targetId);
      const { applyOnKOEffects } = require("./passives");
      next = applyOnKOEffects(next, koOwner, atkOwner, koDefId);
    }
  }

  // Check KO from thunder propagation on adjacent characters
  if (pending.element === "thunder" && !pending.targetIsCaptain) {
    const target = state.cards[pending.targetId];
    if (target?.slot) {
      const adj = getAdjacentSlots(target.slot);
      const opponentId = getOpponent(getAttackerOwner(state, pending.attackerId));
      for (const adjSlot of adj) {
        const adjId = next.players[opponentId].board[adjSlot];
        if (adjId) {
          const adjCard = next.cards[adjId];
          if (adjCard && adjCard.zone === "board" && adjCard.currentPv <= 0) {
            const adjDef = getCardDef(adjCard.defId);
            const adjKoOwner = adjCard.owner;
            const adjKoDefId = adjCard.defId;
            next = addLog(next, adjCard.owner, `${adjDef.name} est KO (foudre) !`);
            // +2 Vol. goes to the player who LOST the ally (Rulebook v3.1 §4).
            next = grantKOBonus(next, adjKoOwner);
            next = removeFromBoard(next, adjId);
            const { applyOnKOEffects: applyAdjKO } = require("./passives");
            next = applyAdjKO(next, adjKoOwner, getAttackerOwner(next, pending.attackerId), adjKoDefId);
          }
          break;
        }
      }
    }
  }

  // On-hit control effects on the surviving primary target.
  if (!pending.targetIsCaptain) {
    const tgt = next.cards[pending.targetId];
    if (tgt && tgt.zone === "board") {
      const tdef = getCardDef(tgt.defId);
      const ctrlImmune = tdef.passive?.effects.some((e) => e.type === "immuneControl") ?? false;
      if (pending.immobilize && !ctrlImmune) {
        next = produce(next, (draft) => {
          draft.cards[pending.targetId].statusEffects.push({ type: "immobilize", turnsRemaining: 2, damagePerTurn: 0, source: pending.attackerId });
        });
        next = addLog(next, tgt.owner, `${tdef.name} est immobilisé !`);
      }
      if (pending.sleep && !ctrlImmune) {
        next = produce(next, (draft) => {
          draft.cards[pending.targetId].statusEffects.push({ type: "sleep", turnsRemaining: 3, damagePerTurn: 0, source: pending.attackerId });
        });
        next = addLog(next, tgt.owner, `${tdef.name} est endormi !`);
      }
      if (pending.stripStealth) {
        next = produce(next, (draft) => {
          draft.cards[pending.targetId].statusEffects.push({ type: "noStealth", turnsRemaining: 2, damagePerTurn: 0, source: pending.attackerId });
        });
      }
      // Decision §8.38 — `noHeal`: the target cannot be healed for 2 turns
      // (`healUnit` skips a unit carrying the status).
      if (pending.noHeal) {
        next = produce(next, (draft) => {
          draft.cards[pending.targetId].statusEffects.push({ type: "noHeal", turnsRemaining: 2, damagePerTurn: 0, source: pending.attackerId });
        });
        next = addLog(next, tgt.owner, `${tdef.name} ne peut plus être soigné !`);
      }
      if (pending.pushback && !(tdef.passive?.effects.some((e) => e.type === "immuneImpact"))) {
        const back: Record<string, string> = { V1: "A1", V2: "A2", V3: "A3" };
        const slot = tgt.slot;
        // Decision §8.37 (`betrayal`): the cell a body occupies belongs to its
        // *controller*, so that is the board the push reads and writes.
        const tgtSide = controllerOf(tgt);
        // §8.31 : la destination de la repoussee est toujours une case A*, et
        // `flipCaptain` accepte les six cases — sans ce test, un capitaine
        // engage en A1 se faisait recouvrir par le personnage repousse.
        if (slot && back[slot] && isSlotFree(next, tgtSide, back[slot] as import("@/types").Slot)) {
          const dest = back[slot];
          next = produce(next, (draft) => {
            const p = draft.players[tgtSide];
            p.board[slot as keyof typeof p.board] = null;
            p.board[dest as keyof typeof p.board] = pending.targetId;
            draft.cards[pending.targetId].slot = dest as import("@/types").Slot;
          });
          next = addLog(next, tgt.owner, `${tdef.name} est repoussé en ${dest} (Impact) !`);
        }
      }
    }
  }

  // Zone / Total spread (Rulebook v3.1 §8 Family 2): after the primary hit, the same
  // attack also strikes the target's adjacents (Zone) or every enemy (Total).
  if (
    !pending.targetIsCaptain &&
    (pending.attackTraits.includes("zone") || pending.attackTraits.includes("total"))
  ) {
    const attackerOwner = getAttackerOwner(state, pending.attackerId);
    const defenderId = getOpponent(attackerOwner);
    const secondaryIds: string[] = [];

    if (pending.attackTraits.includes("total")) {
      for (const c of getBoardCharacters(next, defenderId)) {
        if (c.instanceId !== pending.targetId) secondaryIds.push(c.instanceId);
      }
    } else {
      const primarySlot = state.cards[pending.targetId]?.slot;
      if (primarySlot) {
        for (const adjSlot of getAdjacentSlots(primarySlot)) {
          const adjId = next.players[defenderId].board[adjSlot];
          if (adjId && adjId !== pending.targetId) secondaryIds.push(adjId);
        }
      }
    }

    for (const sid of secondaryIds) {
      next = applySpreadHit(next, pending, sid);
    }
  }

  // Clear pending attack
  next = produce(next, (draft) => {
    draft.pendingAttack = null;
  });

  // Check win condition
  const winner = checkWinCondition(next);
  if (winner) {
    next = produce(next, (draft) => {
      draft.winner = winner;
    });
  }

  return next;
}

/**
 * Apply one Zone/Total spread hit to a single secondary character target.
 * Recomputes ATK - DEF for this target, applies the element, and handles KO.
 */
function applySpreadHit(
  state: GameState,
  pending: PendingAttack,
  targetId: string
): GameState {
  const target = state.cards[targetId];
  if (!target || target.zone !== "board" || target.currentPv <= 0) return state;

  const attackerOwner = getAttackerOwner(state, pending.attackerId);
  const atkPower = pending.attackPower ?? pending.rawDamage;

  // Logia intangibility blocks the spread unless the attack carries Haki/Water.
  if (hasTrait(state, targetId, "logia") && !pending.hasHaki) return state;

  let targetDefVal = getEffectiveDef(state, targetId);
  if (pending.attackTraits.includes("piercing")) {
    targetDefVal = Math.floor(targetDefVal / 2);
  }
  const dmg = Math.max(0, atkPower - targetDefVal);
  const targetDef = getCardDef(target.defId);

  let next = produce(state, (draft) => {
    draft.cards[targetId].currentPv -= dmg;
  });
  next = addLog(
    next,
    attackerOwner,
    `${targetDef.name} subit ${dmg} degats (Zone/Total) (PV: ${next.cards[targetId].currentPv})`
  );

  // Element status on this target (skip thunder re-propagation to avoid chains).
  next = applyElementEffects(next, {
    ...pending,
    targetId,
    targetIsCaptain: false,
    rawDamage: dmg,
    element: pending.element === "thunder" ? undefined : pending.element,
  });

  const after = next.cards[targetId];
  if (after && after.zone === "board" && after.currentPv <= 0) {
    next = addLog(next, after.owner, `${targetDef.name} est KO !`);
    next = grantKOBonus(next, after.owner);
    const koOwner = after.owner;
    const koDefId = after.defId;
    next = removeFromBoard(next, targetId);
    const { applyOnKOEffects } = require("./passives");
    next = applyOnKOEffects(next, koOwner, attackerOwner, koDefId);
  }

  return next;
}

function getAttackerOwner(state: GameState, attackerId: string): PlayerId {
  // Captain attacker IDs look like "captain_player1" or "captain_player2"
  if (attackerId.startsWith("captain_")) {
    return attackerId.replace("captain_", "") as PlayerId;
  }
  const card = state.cards[attackerId];
  if (!card) throw new Error(`Attacker not found: ${attackerId}`);
  // Decision §8.37 (`betrayal`): `controlledBy` first, `owner` after — a
  // borrowed body attacks (and spends Volonte) for whoever controls it.
  return controllerOf(card);
}

function applyCaptainDamage(
  state: GameState,
  pending: PendingAttack & { survivePlayed?: boolean }
): GameState {
  const attackerOwner = getAttackerOwner(state, pending.attackerId);
  const opponentId = getOpponent(attackerOwner);
  const cap = state.players[opponentId].captain;

  // Logia check on captain — decision §8.28 (follow-up): the trait may come
  // from the fruit the captain wears (`MR-011`, `BW-011`), not only from the
  // printed verso list. Rust: `captain::captain_has_trait_now`.
  if (cap.flipped) {
    const capDef = getCaptainDef(cap.defId);
    const { captainHasTraitNow } = require("./captain");
    const isLogia = captainHasTraitNow(state, opponentId, "logia") as boolean;
    // Decision §8.40 — une fois par tour, exactement comme un personnage.
    // Le TS laissait un capitaine Logia ignorer TOUTES les attaques sans Haki
    // du tour : le Rust (reference) compte la premiere et laisse passer les
    // suivantes, avec un test dedie.
    if (isLogia && !pending.hasHaki && pending.rawDamage > 0 && !cap.logiaUsedThisTurn) {
      const marked = produce(state, (draft) => {
        draft.players[opponentId].captain.logiaUsedThisTurn = true;
      });
      return addLog(
        marked,
        attackerOwner,
        `⚠ ${capDef.name} : INTANGIBILITE LOGIA ! L'attaque passe a travers. Utilisez le Haki (T7+) ou l'Eau pour le toucher.`
      );
    }
  }

  let damage = pending.rawDamage;

  return produce(state, (draft) => {
    const targetCap = draft.players[opponentId].captain;
    targetCap.currentPv -= damage;
    if (pending.survivePlayed && targetCap.currentPv <= 0) {
      targetCap.currentPv = 1;
    }
    draft.log.push({
      turn: draft.turnNumber,
      player: attackerOwner,
      message: `Capitaine ${getCaptainDef(targetCap.defId).name} subit ${damage} degats (PV: ${targetCap.currentPv})`,
    });
  });
}

function applyCharacterDamage(
  state: GameState,
  pending: PendingAttack & { survivePlayed?: boolean }
): GameState {
  const target = state.cards[pending.targetId];
  if (!target) return state;

  const targetDef = getCardDef(target.defId);

  // Decision §8.67 — MG-018 Dial d'Impact : arme, il absorbe entierement la
  // prochaine attaque subie et la renvoie a l'attaquant. Le texte imprime
  // renvoie « a votre prochain tour » ; la detente est ramenee a l'instant de
  // l'absorption, faute de quoi le montant devrait survivre a un changement de
  // tour ET a une seconde selection de cible — compression assumee et
  // consignee, le montant et la cible restant ceux du texte.
  const reflect = target.statusEffects.find((e) => e.type === "reflect");
  if (reflect && pending.rawDamage > 0) {
    const amount = pending.rawDamage;
    const attackerOwner = getAttackerOwner(state, pending.attackerId);
    let next = produce(state, (draft) => {
      const t = draft.cards[pending.targetId];
      t.statusEffects = t.statusEffects.filter((e) => e.type !== "reflect");
      const a = draft.cards[pending.attackerId];
      if (a) a.currentPv -= amount;
    });
    next = addLog(next, target.owner, `Dial d'Impact : ${amount} degats absorbes puis renvoyes !`);
    if (state.cards[pending.attackerId]) {
      const an = getCardDef(state.cards[pending.attackerId].defId).name;
      next = addLog(next, attackerOwner, `${an} encaisse ${amount} degats (Impact).`);
    }
    return next;
  }

  // Logia check (includes traits from equipped Devil Fruits)
  const isLogia = hasTrait(state, pending.targetId, "logia");
  if (isLogia && !pending.hasHaki && pending.rawDamage > 0) {
    // Check if logia already used this turn
    if (!target.logiaUsedThisTurn) {
      let next = produce(state, (draft) => {
        draft.cards[pending.targetId].logiaUsedThisTurn = true;
      });
      return addLog(
        next,
        getAttackerOwner(state, pending.attackerId),
        `⚠ ${targetDef.name} : INTANGIBILITE LOGIA ! Utilisez le Haki (T7+) ou l'Eau.`
      );
    }
  }

  let damage = pending.rawDamage;

  let next = produce(state, (draft) => {
    const t = draft.cards[pending.targetId];
    t.currentPv -= damage;
    if (pending.survivePlayed && t.currentPv <= 0) {
      t.currentPv = 1;
    }
  });

  const attackerOwner = getAttackerOwner(state, pending.attackerId);
  next = addLog(
    next,
    attackerOwner,
    `${targetDef.name} subit ${damage} degats (PV: ${next.cards[pending.targetId].currentPv})`
  );

  // Check KO
  if (next.cards[pending.targetId].currentPv <= 0) {
    // Chapeau de Paille (RH-014): the first time the bearer would be KO'd, it survives at 1 PV.
    const bearer = next.cards[pending.targetId];
    const strawhat = bearer.attachedObjects.find((id) => next.cards[id]?.defId === "RH-014");
    if (strawhat && !bearer.usedOnceAbilities.includes("strawhat")) {
      next = produce(next, (draft) => {
        const b = draft.cards[pending.targetId];
        b.currentPv = 1;
        b.usedOnceAbilities.push("strawhat");
      });
      return addLog(next, target.owner, `${targetDef.name} survit grâce au Chapeau de Paille (1 PV) !`);
    }
    next = addLog(next, target.owner, `${targetDef.name} est KO !`);
    // +2 Vol. goes to the player who LOST the ally (Rulebook v3.1 §4), not the attacker.
    next = grantKOBonus(next, target.owner);
    const koDefId = target.defId;
    const koOwner = target.owner;
    next = removeFromBoard(next, pending.targetId);
    // Apply on-KO effects (captain bonuses, synergy rage, recalculate buffs)
    const { applyOnKOEffects } = require("./passives");
    next = applyOnKOEffects(next, koOwner, attackerOwner, koDefId);
  }

  return next;
}

/**
 * Apply element effects after damage.
 */
function applyElementEffects(
  state: GameState,
  pending: PendingAttack
): GameState {
  if (!pending.element) return state;
  if (pending.targetIsCaptain) {
    // Apply element to captain
    return applyElementToCaptain(state, pending);
  }

  const target = state.cards[pending.targetId];
  if (!target || target.zone !== "board") return state;

  let next = state;

  switch (pending.element) {
    case "fire":
      // Burn: 1 dmg/turn for 2 turns
      next = produce(next, (draft) => {
        draft.cards[pending.targetId].statusEffects.push({
          type: "burn",
          turnsRemaining: 2,
          damagePerTurn: 1,
          source: pending.attackerId,
        });
      });
      break;

    case "ice":
      // Freeze: target loses next action
      // turnsRemaining: 2 so it survives the start-of-turn decrement and blocks for 1 full turn
      next = produce(next, (draft) => {
        draft.cards[pending.targetId].statusEffects.push({
          type: "freeze",
          turnsRemaining: 2,
          damagePerTurn: 0,
          source: pending.attackerId,
        });
      });
      break;

    case "thunder":
      // Propagate damage to 1 adjacent
      if (target.slot) {
        const adj = getAdjacentSlots(target.slot);
        const opponent = getOpponent(getAttackerOwner(state, pending.attackerId));
        for (const adjSlot of adj) {
          const adjId = state.players[opponent].board[adjSlot];
          if (adjId) {
            const propagateDmg = Math.max(1, Math.floor(pending.rawDamage / 2));
            next = produce(next, (draft) => {
              const adjCard = draft.cards[adjId];
              if (adjCard) {
                adjCard.currentPv -= propagateDmg;
              }
            });
            // Only propagate to 1
            break;
          }
        }
      }
      break;

    case "poison":
      // 1 dmg/turn permanent, can't kill
      next = produce(next, (draft) => {
        draft.cards[pending.targetId].statusEffects.push({
          type: "poison",
          turnsRemaining: -1, // permanent
          damagePerTurn: 1,
          source: pending.attackerId,
        });
      });
      break;

    case "sand":
      // -1 PV permanent (irrecoverable)
      next = produce(next, (draft) => {
        draft.cards[pending.targetId].currentPv -= 1;
      });
      break;

    case "water":
      // x2 damage vs Cursed — already factored into raw damage at declaration time?
      // For now, apply bonus damage if target is Cursed
      if (hasTrait(state, pending.targetId, "cursed")) {
        next = produce(next, (draft) => {
          const t = draft.cards[pending.targetId];
          if (t && t.zone === "board") {
            t.currentPv -= pending.rawDamage; // double = apply again
          }
        });
      }
      break;
  }

  return next;
}

function applyElementToCaptain(
  state: GameState,
  pending: PendingAttack
): GameState {
  const attackerOwner = getAttackerOwner(state, pending.attackerId);
  const opponentId = getOpponent(attackerOwner);

  let next = state;

  switch (pending.element) {
    case "fire":
      next = produce(next, (draft) => {
        draft.players[opponentId].captain.statusEffects.push({
          type: "burn",
          turnsRemaining: 2,
          damagePerTurn: 1,
          source: pending.attackerId,
        });
      });
      break;
    case "ice":
      next = produce(next, (draft) => {
        draft.players[opponentId].captain.statusEffects.push({
          type: "freeze",
          // 2 turns so it survives the start-of-turn decrement and actually
          // skips the captain's next action (matches character freeze).
          turnsRemaining: 2,
          damagePerTurn: 0,
          source: pending.attackerId,
        });
      });
      break;
    // Other elements on captain — simplified for MVP
  }

  return next;
}

// ============================================================
// Helpers for getting eligible counters
// ============================================================

/**
 * Get counter cards in hand that can be played during the counter window.
 */
export function getEligibleCounters(
  state: GameState,
  playerId: PlayerId
): string[] {
  if (!state.pendingAttack) return [];

  const player = state.players[playerId];
  return player.hand.filter((id) => {
    const card = state.cards[id];
    const def = getCardDef(card.defId);
    if (def.type !== "counter" || !canAfford(state, playerId, def.cost)) return false;
    const ce = def.counterEffect;
    if (ce?.type === "cancel" && ce.maxAttackerAtk !== undefined) {
      return (state.pendingAttack?.attackPower ?? 0) <= ce.maxAttackerAtk;
    }
    // Decision §8.57: `getEligibleCounters` is the legality contract for the UI
    // and the AI — an offered counter must not throw. `survive` protects an
    // ALLY CHARACTER, once per character (§8.16), so it is screened out against
    // a captain-targeted attack and against a body that already saved itself.
    if (ce?.type === "survive") {
      if (state.pendingAttack?.targetIsCaptain) return false;
      const protectedCard = state.cards[state.pendingAttack!.targetId];
      if (protectedCard?.usedOnceAbilities.includes("survived")) return false;
    }
    return true;
  });
}

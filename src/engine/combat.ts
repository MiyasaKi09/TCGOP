import { produce } from "immer";
import type {
  GameState,
  PlayerId,
  PendingAttack,
  AttackTrait,
  Element,
  CardInstance,
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
} from "./board";
import { spendVolonte, canAfford, grantKOBonus } from "./volonte";
import { addLog, getOpponent, checkWinCondition } from "./gameState";

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
  return false;
}

/** Whether the attacker strips Furtif from targets it hits (Smoker). */
function attackerStripsStealth(state: GameState, attackerInstanceId: string): boolean {
  const card = state.cards[attackerInstanceId];
  if (!card) return false;
  return getCardDef(card.defId).passive?.effects.some((e) => e.type === "stripStealthOnAttack") ?? false;
}

/** Conditional ATK bonus from a special when the target matches a trait/faction. */
function conditionalAtkBonus(
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

  const baseAction = def.baseAction;
  const atk = getEffectiveAtk(state, attackerInstanceId);
  const attackTraits: AttackTrait[] = baseAction?.attackTraits ?? [];

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
    const opponent = getOpponent(attacker.owner);
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
    (def.naturalHaki && def.naturalHaki.length > 0) ||
    state.turnNumber >= 7 ||
    attackElement === "water" ||
    !!state.players[attacker.owner].hakiThisTurn;

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
    attacker.owner,
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

  // Check 1x/game
  if (spec.oncePerGame && attacker.usedOnceAbilities.includes(spec.name)) {
    throw new Error("Already used this ability (1x/game)");
  }

  if (!canAfford(state, attacker.owner, spec.cost)) {
    throw new Error(`Cannot afford special (cost ${spec.cost})`);
  }

  // Self-transformation special (Chopper Monster Point): no target, no pending attack.
  if (spec.transform) {
    const t = spec.transform;
    let tnext = spendVolonte(state, attacker.owner, spec.cost);
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
    return addLog(tnext, attacker.owner, `${def.name} : ${spec.name} ! ATK ${t.atk} pendant ${t.turns} tours, puis KO.`);
  }

  let next = spendVolonte(state, attacker.owner, spec.cost);

  const baseAtk = getEffectiveAtk(state, attackerInstanceId);
  const condBonus = conditionalAtkBonus(state, spec.conditionalBonus, targetInstanceId, targetIsCaptain, getOpponent(attacker.owner));
  const totalAtk = baseAtk + spec.atkBonus + condBonus;

  // "Touche 2 cibles" is approximated as a small Zone (target + adjacents).
  const attackTraits: AttackTrait[] = [...(spec.attackTraits ?? []), ...(spec.twoTargets && !(spec.attackTraits ?? []).includes("zone") ? ["zone" as AttackTrait] : [])];

  let targetDefVal = 0;
  if (targetIsCaptain) {
    const opponent = getOpponent(attacker.owner);
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
    (def.naturalHaki && def.naturalHaki.length > 0) ||
    state.turnNumber >= 7 ||
    spec.element === "water" ||
    !!state.players[attacker.owner].hakiThisTurn;

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
    ignoreShield: spec.ignoreShield,
    immobilize: spec.immobilize,
    sleep: spec.sleep,
    pushback: spec.pushback || (spec.pushbackSlots ?? 0) > 0,
    stripStealth: spec.stripStealth || attackerStripsStealth(state, attackerInstanceId),
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
    attacker.owner,
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
  const attacker = state.cards[attackerInstanceId];
  if (!attacker) throw new Error("Attacker not found");
  if (hasSummoningSickness(state, attackerInstanceId)) {
    throw new Error("Character has summoning sickness");
  }

  const fruitCard = state.cards[fruitInstanceId];
  if (!fruitCard || !fruitCard.isAwakened) throw new Error("Fruit not awakened");

  const fruitDef = getCardDef(fruitCard.defId);
  const spec = fruitDef.fruitEffects?.awakening?.specialAttack;
  if (!spec) throw new Error("Fruit has no awakening special attack");

  // Check once per game
  if (spec.oncePerGame && attacker.usedOnceAbilities.includes(spec.name)) {
    throw new Error("Already used this fruit ability (1x/game)");
  }

  if (!canAfford(state, attacker.owner, spec.cost)) {
    throw new Error(`Cannot afford fruit special (cost ${spec.cost})`);
  }

  let next: GameState = spendVolonte(state, attacker.owner, spec.cost);

  const def = getCardDef(attacker.defId);
  const baseAtk = getEffectiveAtk(next, attackerInstanceId);
  const totalAtk = baseAtk + spec.atkBonus;

  const attackTraits: AttackTrait[] = spec.attackTraits ?? [];

  let targetDefVal = 0;
  if (targetIsCaptain) {
    const opponent = getOpponent(attacker.owner);
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
    (def.naturalHaki && def.naturalHaki.length > 0) || next.turnNumber >= 7 || spec.element === "water";

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
    ignoreShield: spec.ignoreShield,
    immobilize: spec.immobilize,
    sleep: spec.sleep,
    pushback: spec.pushback,
    stripStealth: spec.stripStealth,
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
    attacker.owner,
    `${def.name} utilise ${spec.name} sur ${targetName} (ATK ${totalAtk} vs DEF ${targetDefVal} = ${rawDamage} degats)`
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

  let next = spendVolonte(state, owner, cdef.cost);
  next = produce(next, (draft) => {
    const p = draft.players[owner];
    p.hand = p.hand.filter((id) => id !== counterInstanceId);
    draft.cards[counterInstanceId].zone = "graveyard";
    p.graveyard.push(counterInstanceId);
    draft.pendingAttack = null;
  });
  next = addLog(next, owner, `${cdef.name} : attaque annulée !`);

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
      if (pending.pushback && !(tdef.passive?.effects.some((e) => e.type === "immuneImpact"))) {
        const back: Record<string, string> = { V1: "A1", V2: "A2", V3: "A3" };
        const slot = tgt.slot;
        if (slot && back[slot] && next.players[tgt.owner].board[back[slot] as keyof typeof next.players.player1.board] === null) {
          const dest = back[slot];
          next = produce(next, (draft) => {
            const p = draft.players[tgt.owner];
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
  return card.owner;
}

function applyCaptainDamage(
  state: GameState,
  pending: PendingAttack & { survivePlayed?: boolean }
): GameState {
  const attackerOwner = getAttackerOwner(state, pending.attackerId);
  const opponentId = getOpponent(attackerOwner);
  const cap = state.players[opponentId].captain;

  // Logia check on captain
  if (cap.flipped) {
    const capDef = getCaptainDef(cap.defId);
    const isLogia = capDef.verso.traits?.includes("logia") ?? false;
    if (isLogia && !pending.hasHaki && pending.rawDamage > 0) {
      return addLog(
        state,
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
          turnsRemaining: 1,
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
    return true;
  });
}

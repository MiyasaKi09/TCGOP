import { produce } from "immer";
import type {
  GameState,
  PlayerId,
  Slot,
  EntryEffect,
  Trait,
  CaptainInstance,
  SpecialAttack,
  AttackTrait,
  PendingAttack,
} from "@/types";
import { getCaptainDef, getCardDef } from "./cardRegistry";
import { canAfford, spendVolonte } from "./volonte";
import { addLog, getOpponent, checkWinCondition } from "./gameState";
import {
  getBoardCharacters,
  getEffectiveAtk,
  getEffectiveDef,
  hasTrait,
  attachmentsGrantTrait,
  attachmentsGrantedAttackTraits,
  moveAttachedObjectsInDraft,
} from "./board";

export type FreeFlipReason =
  | "allyKO"
  | "autoIfAlliesLte"
  | "enemyCursed"
  | "alliesGte"
  | "turnGte";

/**
 * A frozen / immobilized / sleeping captain cannot act — the single predicate
 * shared by the enumerator and the executors (decision §8.1 item 35).
 * Rust: `captain::captain_cannot_act`.
 */
export function captainCannotAct(captain: CaptainInstance): boolean {
  return captain.statusEffects.some(
    (e) => e.type === "freeze" || e.type === "immobilize" || e.type === "sleep"
  );
}

/**
 * Decision §8.28 (follow-up) × §8.40 — the captain's *live* traits: its printed
 * traits (card level, plus the verso list once flipped) **plus** the traits its
 * equipment grants.
 *
 * This is `hasTrait` for a captain, and it exists for the same reason: once the
 * captain can wear the fruit printed for it ("Équipable sur Luffy / Crocodile /
 * Akainu"), that fruit's `grantsTraits` — `logia` and `cursed` on `BW-011` /
 * `MR-011`, `cursed` on `MG-014`, plus the awakening list once awakened —
 * belong to the captain exactly as they belong to a character. With no
 * attachment it is the printed read, unchanged.
 * Rust: `captain::captain_has_trait_now`.
 */
export function captainHasTraitNow(
  state: GameState,
  playerId: PlayerId,
  trait: Trait
): boolean {
  const captain = state.players[playerId].captain;
  const def = getCaptainDef(captain.defId);
  if (def.traits?.includes(trait)) return true;
  if (captain.flipped && def.verso.traits?.includes(trait)) return true;
  return attachmentsGrantTrait(state, captain.attachedObjects ?? [], trait);
}

/**
 * Is an enemy Cursed unit in play? (`freeIfEnemyCursed`, decision §8.33.)
 *
 * Any enemy board character carrying the `cursed` trait (equipment included,
 * via `hasTrait`) or an enemy captain whose **active** traits are Cursed —
 * decision §8.40: `captainTraits(def, flipped) = def.traits ∪ (flipped ? verso.traits : [])`,
 * so a card-level `cursed` stays visible once the captain flips.
 * Rust: `captain::enemy_cursed_in_play`.
 */
function enemyCursedInPlay(state: GameState, playerId: PlayerId): boolean {
  const opponentId = getOpponent(playerId);
  for (const c of getBoardCharacters(state, opponentId)) {
    if (hasTrait(state, c.instanceId, "cursed")) return true;
  }
  // Decision §8.28 (follow-up): a captain wearing a Cursed fruit is a Cursed
  // unit in play, exactly like a character wearing one.
  return captainHasTraitNow(state, opponentId, "cursed");
}

/**
 * The single free-flip predicate shared by `canFlipCaptain` and `flipCaptain`
 * (decision §8.33) — the five `flipCondition` clauses tested in declaration
 * order. Rust: `captain::free_flip_reason`.
 */
export function freeFlipReason(
  state: GameState,
  playerId: PlayerId
): FreeFlipReason | null {
  const captain = state.players[playerId].captain;
  const condition = getCaptainDef(captain.defId).flipCondition;
  const allyCount = getBoardCharacters(state, playerId).length;

  // Free flip if a Mugiwara ally was KO'd this turn (Luffy).
  if (condition.freeIfAllyKO && state.players[playerId].allyKOedThisTurn) return "allyKO";

  // Auto-flip condition (allies <= N) — only from turn 4+ to prevent early abuse.
  if (
    condition.autoIfAlliesLte !== undefined &&
    state.turnNumber >= 4 &&
    allyCount <= condition.autoIfAlliesLte
  ) {
    return "autoIfAlliesLte";
  }

  // Akainu: free while a Cursed enemy is in play.
  if (condition.freeIfEnemyCursed && enemyCursedInPlay(state, playerId)) return "enemyCursed";

  // Crocodile: free once the Baroque Works board is wide enough.
  if (condition.freeIfAlliesGte !== undefined && allyCount >= condition.freeIfAlliesGte) {
    return "alliesGte";
  }

  // Shanks: free from the printed turn onwards.
  if (condition.freeIfTurnGte !== undefined && state.turnNumber >= condition.freeIfTurnGte) {
    return "turnGte";
  }

  return null;
}

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

  // Decision §8.33: one shared free-flip predicate with the executor.
  if (freeFlipReason(state, playerId) !== null) return true;

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

  // Determine cost — one shared predicate with `canFlipCaptain` (§8.33), which
  // also honours `freeIfEnemyCursed` (Akainu), `freeIfAlliesGte` (Crocodile)
  // and `freeIfTurnGte` (Shanks).
  let cost = 0;
  const freeFlip = freeFlipReason(state, playerId) !== null;

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
    // Decision §8.28 (follow-up) × §8.29: the equipment stands where its bearer
    // stands, so a captain equipped while still recto (off-board, slot
    // undefined) brings its objects into the slot it flips into.
    moveAttachedObjectsInDraft(draft, cap.attachedObjects ?? [], slot);
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
          const cursed = getCardDef(next.cards[tid].defId).traits?.includes("cursed") ?? false;
          let dmg = cursed && effect.cursedBonus ? effect.cursedBonus : effect.amount;
          if (effect.sand) dmg += 1; // permanent PV loss approximated
          next = produce(next, (draft) => { draft.cards[tid].currentPv -= dmg; });
          const td = getCardDef(next.cards[tid].defId);
          next = addLog(next, playerId, `Effet d'entree : ${dmg} degats a ${td.name} !`);
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

    case "haoshoku": {
      // Shanks: immobilize enemies of DEF <= N and give all enemies -ATK this turn.
      const opponentId = getOpponent(playerId);
      next = produce(next, (draft) => {
        for (const slot of Object.values(draft.players[opponentId].board)) {
          if (!slot) continue;
          const c = draft.cards[slot];
          if (!c) continue;
          const d = getCardDef(c.defId);
          c.modifiers.push({ id: `haoshoku_${slot}_${Date.now()}`, stat: "atk", amount: -effect.debuffAtk, source: "entry_haoshoku", duration: "turn" });
          const ctrlImmune = d.passive?.effects.some((e) => e.type === "immuneControl") ?? false;
          if (!ctrlImmune && (d.def ?? 0) <= effect.immobilizeMaxDef) {
            c.statusEffects.push({ type: "immobilize", turnsRemaining: 2, damagePerTurn: 0, source: "haoshoku" });
          }
        }
      });
      next = addLog(next, playerId, `Effet d'entree : Haoshoku Haki !`);
      break;
    }

    case "debuffAllEnemies": {
      const opponentId = getOpponent(playerId);
      next = produce(next, (draft) => {
        for (const slot of Object.values(draft.players[opponentId].board)) {
          if (!slot) continue;
          const c = draft.cards[slot];
          if (c) c.modifiers.push({ id: `entrydebuff_${slot}_${Date.now()}`, stat: "atk", amount: -effect.atk, source: "entry", duration: "turn" });
        }
      });
      break;
    }

    case "discardOpponentRandom": {
      const opponentId = getOpponent(playerId);
      next = produce(next, (draft) => {
        for (let i = 0; i < effect.amount && draft.players[opponentId].hand.length > 0; i++) {
          const h = draft.players[opponentId].hand;
          const idx = Math.floor(Math.random() * h.length);
          const [d2] = h.splice(idx, 1);
          draft.cards[d2].zone = "graveyard";
          draft.players[opponentId].graveyard.push(d2);
        }
      });
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
  // Decision §8.1 item 35 : un capitaine gelé / immobilisé / endormi n'agit
  // pas. L'énumérateur le savait déjà, l'exécuteur non — ses deux voisins
  // (`declareCaptainSpecAttack`, `declareCaptainFruitSpecialAttack`) le
  // vérifient. Rust : `captain::declare_captain_base_attack`.
  if (captainCannotAct(captain)) throw new Error(CAPTAIN_CANNOT_ACT);

  const def = getCaptainDef(captain.defId);
  const baseAction = def.verso.baseAction;

  // Decision §8.38: a taunted attacker may only declare against its taunter.
  const { enforceTaunt } = require("./combat");
  enforceTaunt(state, `captain_${playerId}`, targetInstanceId, targetIsCaptain, false);

  // Captain summoning sickness
  if (captain.deployedTurn === state.turnNumber) {
    // Verso captain just flipped — has mal de terre unless Rush
    // Decision §8.28 (follow-up) × §8.40: every captain trait read goes through
    // the union helper, `rush` included — an awakened Gomu Gomu no Mi grants it.
    const hasRush = captainHasTraitNow(state, playerId, "rush");
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

  // Decision §8.47 × §8.28 : le bras `AttackTrait` de `grantsTraits` des objets
  // que porte le capitaine rejoint son attaque de base, comme pour un personnage.
  const baseAttackTraits: AttackTrait[] = [...(baseAction.attackTraits ?? [])];
  for (const at of attachmentsGrantedAttackTraits(state, captain.attachedObjects ?? [])) {
    if (!baseAttackTraits.includes(at)) baseAttackTraits.push(at);
  }

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
      attackTraits: baseAttackTraits,
      hasHaki,
    };
  });

  return addLog(
    next,
    playerId,
    `Capitaine ${def.name} attaque avec ${baseAction.name} ! (${rawDamage} degats)`
  );
}

// ============================================================
// Decision §8.34 — the captain's special attack and its surcharge
// ============================================================

/**
 * The `usedOnceAbilities` key guarding a `oncePerGame` captain surcharge named
 * `name` (decision §8.34/§8.50). Rust: `state::once_surcharge`.
 */
export function onceSurchargeKey(name: string): string {
  return `surcharge_${name}`;
}

/** The error a frozen / immobilized / sleeping captain gets (decision §8.1 item 35). */
const CAPTAIN_CANNOT_ACT = "Captain cannot act (frozen, immobilized or asleep)";

/**
 * The captain's signature move — decision §8.34(a).
 *
 * Dispatched from the existing `captainAttack` action with `isSpecial: true`
 * (the field was already on the wire, so no new action type is introduced).
 * It pays `verso.specialAttack.cost`, taps the captain, sets both action flags
 * and builds the pending attack exactly like a character special: `atkBonus`,
 * `element`, `attackTraits` (+ `zone` for `twoTargets`), `conditionalBonus`,
 * piercing, `ignoreDef`, `ignoreShield`, `immobilize`, `sleep`,
 * `cannotBeDodged`, `pushback`, `stripStealth` and `oncePerGame` (recorded
 * under the attack name in `captain.usedOnceAbilities`).
 *
 * `permanentPvLoss` (Crocodile's Desert Girasol, −2 PV) and `noHeal` ride on
 * the pending attack (decision §8.38) and are applied by `resolveAttack` once
 * the blow lands — they are never folded into the raw damage, where a counter
 * could have reduced them away.
 *
 * Rust: `captain::declare_captain_special_attack`.
 */
export function declareCaptainSpecialAttack(
  state: GameState,
  playerId: PlayerId,
  targetInstanceId: string,
  targetIsCaptain: boolean
): GameState {
  const captain = state.players[playerId].captain;
  const def = getCaptainDef(captain.defId);
  const spec = def.verso.specialAttack;
  return declareCaptainSpecAttack(
    state,
    playerId,
    spec,
    spec.oncePerGame ? spec.name : undefined,
    targetInstanceId,
    targetIsCaptain
  );
}

/**
 * The `useSurcharge` action — decision §8.34(b).
 *
 * Generic, data-driven plumbing for the `surcharge` block of the captain's
 * **active** face: it resolves through the same code path as
 * `declareCaptainSpecialAttack` and costs `surcharge.cost`. Inert on the
 * shipped catalogue (every face has no `surcharge`).
 *
 * The active face is always the verso: the surcharge is an attack and "Le
 * Capitaine recto ne peut pas attaquer" (Rulebook v3.1 §2.1, quoted verbatim
 * in `captains.ts`), so a recto captain is refused before its face is read —
 * a `recto.surcharge` could never be played.
 *
 * The one-per-turn limit falls out of the shared per-turn flags
 * (`tapped` / `usedSpecialAttack`); the `surcharge_{name}` key in
 * `captain.usedOnceAbilities` — that list is never cleared — guards a
 * `oncePerGame` surcharge.
 *
 * Rust: `captain::use_captain_surcharge`.
 */
export function useCaptainSurcharge(
  state: GameState,
  playerId: PlayerId,
  targetInstanceId: string,
  targetIsCaptain: boolean
): GameState {
  const captain = state.players[playerId].captain;
  if (!captain.flipped) throw new Error("Captain not flipped (verso required)");
  const def = getCaptainDef(captain.defId);
  const surcharge = def.verso.surcharge;
  if (!surcharge) throw new Error("Captain face has no surcharge");
  return declareCaptainSpecAttack(
    state,
    playerId,
    surcharge,
    surcharge.oncePerGame ? onceSurchargeKey(surcharge.name) : undefined,
    targetInstanceId,
    targetIsCaptain
  );
}

/**
 * The shared body of `declareCaptainSpecialAttack` and `useCaptainSurcharge` —
 * a captain attack driven by a `SpecialAttack` block.
 * Rust: `captain::declare_captain_spec_attack`.
 */
function declareCaptainSpecAttack(
  state: GameState,
  playerId: PlayerId,
  spec: SpecialAttack,
  onceKey: string | undefined,
  targetInstanceId: string,
  targetIsCaptain: boolean
): GameState {
  const captain = state.players[playerId].captain;
  if (!captain.flipped) throw new Error("Captain not flipped (verso required)");
  if (captain.tapped) throw new Error("Captain is tapped");
  if (captain.usedSpecialAttack) throw new Error("Captain special already used");
  if (captainCannotAct(captain)) throw new Error(CAPTAIN_CANNOT_ACT);

  const def = getCaptainDef(captain.defId);

  // Captain summoning sickness — same rule as the base action (§8.40: the
  // trait read goes through the union helper, `rush` included).
  if (
    captain.deployedTurn === state.turnNumber &&
    !captainHasTraitNow(state, playerId, "rush")
  ) {
    throw new Error("Captain has summoning sickness");
  }

  if (onceKey !== undefined && captain.usedOnceAbilities.includes(onceKey)) {
    throw new Error("Already used this ability (1x/game)");
  }
  if (!canAfford(state, playerId, spec.cost)) {
    throw new Error(`Cannot afford captain special (cost ${spec.cost})`);
  }

  // Decision §8.38: the taunt binds the captain's special too (inert while
  // nothing in the shipped catalogue can taunt a captain).
  const { enforceTaunt, conditionalAtkBonus } = require("./combat");
  enforceTaunt(state, `captain_${playerId}`, targetInstanceId, targetIsCaptain, true);

  const facePiercing = captainHasTraitNow(state, playerId, "piercing");
  const hasNaturalHaki = (def.verso.naturalHaki?.length ?? 0) > 0;

  // Base ATK = the verso stat plus the captain's ATK modifiers, then the
  // attack's own bonus (the character path does exactly this).
  let baseAtk = def.verso.atk;
  for (const mod of captain.modifiers) {
    if (mod.stat === "atk") baseAtk += mod.amount;
  }
  const condBonus: number = conditionalAtkBonus(
    state,
    spec.conditionalBonus,
    targetInstanceId,
    targetIsCaptain,
    getOpponent(playerId)
  );
  const totalAtk = baseAtk + spec.atkBonus + condBonus;

  // "Touche 2 cibles" is approximated as a small Zone, like the character special.
  const attackTraits: AttackTrait[] = [...(spec.attackTraits ?? [])];
  if (spec.twoTargets && !attackTraits.includes("zone")) attackTraits.push("zone");
  // Decision §8.47 × §8.28.
  for (const at of attachmentsGrantedAttackTraits(state, state.players[playerId].captain.attachedObjects ?? [])) {
    if (!attackTraits.includes(at)) attackTraits.push(at);
  }

  let targetDefVal = 0;
  if (targetIsCaptain) {
    const opponentId = getOpponent(playerId);
    const oppCap = state.players[opponentId].captain;
    const oppCapDef = getCaptainDef(oppCap.defId);
    targetDefVal = oppCap.flipped ? oppCapDef.verso.def : oppCapDef.recto.def;
    for (const mod of oppCap.modifiers) {
      if (mod.stat === "def") targetDefVal += mod.amount;
    }
    // Decision §8.55: DEF is a non-negative stat — a captain debuffed below 0
    // must not *gain* damage through the piercing `floor(def / 2)`.
    targetDefVal = Math.max(0, targetDefVal);
  } else {
    targetDefVal = getEffectiveDef(state, targetInstanceId);
  }
  if (attackTraits.includes("piercing") || facePiercing) {
    targetDefVal = Math.floor(Math.max(0, targetDefVal) / 2);
  }
  // TS truthiness: `ignoreDef: 0` is falsy — no clamp at all.
  if (spec.ignoreDef) targetDefVal = Math.max(0, targetDefVal - spec.ignoreDef);

  const rawDamage = Math.max(0, totalAtk - targetDefVal);

  // Haki to pierce Logia: natural Haki, Armament passive (T7+), Water element
  // or a Haki granted this turn (Rulebook v3.1 §7/§9).
  const hasHaki =
    hasNaturalHaki ||
    state.turnNumber >= 7 ||
    spec.element === "water" ||
    !!state.players[playerId].hakiThisTurn;

  let next = spendVolonte(state, playerId, spec.cost);

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
    ignoreShield: spec.ignoreShield,
    cannotBeDodged: spec.cannotBeDodged,
    immobilize: spec.immobilize,
    sleep: spec.sleep,
    pushback: spec.pushback || (spec.pushbackSlots ?? 0) > 0,
    stripStealth: spec.stripStealth,
    // Decision §8.38: "La cible perd N PV permanent (Sable)" / "… et ne peut
    // plus etre soignee" ride on the pending attack and resolve after damage.
    permanentPvLoss: spec.permanentPvLoss,
    noHeal: spec.noHeal,
  };

  next = produce(next, (draft) => {
    const cap = draft.players[playerId].captain;
    cap.tapped = true;
    // One action per turn (Rulebook v3.1 §2.2/§6): base OR special, never both.
    cap.usedSpecialAttack = true;
    cap.usedBaseAction = true;
    if (onceKey !== undefined) cap.usedOnceAbilities.push(onceKey);
    draft.pendingAttack = pending;
  });

  const targetName = targetIsCaptain
    ? "Capitaine"
    : getCardDef(next.cards[targetInstanceId].defId).name;
  return addLog(
    next,
    playerId,
    `Capitaine ${def.name} utilise ${spec.name} sur ${targetName} (ATK ${totalAtk} vs DEF ${targetDefVal} = ${rawDamage} degats)`
  );
}

import { produce } from "immer";
import type { GameState, PlayerId, Slot, EntryEffect, Trait, CaptainInstance } from "@/types";
import { getCaptainDef, getCardDef } from "./cardRegistry";
import { canAfford, spendVolonte } from "./volonte";
import { addLog, getOpponent, checkWinCondition } from "./gameState";
import {
  getBoardCharacters,
  getEffectiveAtk,
  getEffectiveDef,
  hasTrait,
  attachmentsGrantTrait,
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
      attackTraits: baseAction.attackTraits ?? [],
      hasHaki,
    };
  });

  return addLog(
    next,
    playerId,
    `Capitaine ${def.name} attaque avec ${baseAction.name} ! (${rawDamage} degats)`
  );
}

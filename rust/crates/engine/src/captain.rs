//! Captains — port of `src/engine/captain.ts`.
//!
//! A captain starts *recto*: off-board, unable to attack, contributing only its
//! passive. Flipping it (irreversibly) puts the *verso* face into a board slot,
//! carries the marked damage over (`versoPv - (rectoPv - currentPv)`), triggers
//! the verso `entryEffect` and unlocks the captain's base attack.
#![allow(unused)]

use crate::context::EngineContext;
use crate::error::EngineError;
use crate::registry::CardRegistry;
use crate::state::GameState;
use crate::types::{EntryEffect, PlayerId, Slot};

/// TS `canFlipCaptain(state, playerId)` — `src/engine/captain.ts:11`.
///
/// `false` once flipped. Then, in order: a free flip when
/// `freeIfAllyKO && allyKOedThisTurn`; a free flip when
/// `autoIfAlliesLte !== undefined && turnNumber >= 4 && allyCount <= autoIfAlliesLte`;
/// otherwise `canAfford(condition.cost)` when a cost exists, else `false`.
///
/// Note the TS engine never reads `freeIfEnemyCursed`, `freeIfAlliesGte` or
/// `freeIfTurnGte` — the port must ignore them too.
pub fn can_flip_captain(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Result<bool, EngineError> {
    todo!("PORT: canFlipCaptain")
}

/// TS `flipCaptain(state, playerId, slot)` — `src/engine/captain.ts:44`.
///
/// Recomputes the free-flip test, pays `condition.cost` otherwise, sets
/// `flipped`, `currentPv = verso.pv - max(0, recto.pv - currentPv)`, `slot`
/// and `deployedTurn`, logs
/// `"{name} s'engage sur le champ de bataille ! (verso, slot {slot})"`,
/// checks the win condition (flipping into a smaller face can be lethal — the
/// function returns **before** the entry effect in that case) and finally runs
/// [`resolve_entry_effect`].
///
/// Errors: `Captain already flipped`, `Slot {slot} is occupied`,
/// `Cannot afford captain flip`.
pub fn flip_captain(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    slot: Slot,
) -> Result<(), EngineError> {
    todo!("PORT: flipCaptain")
}

/// TS `resolveEntryEffect(state, playerId, effect)` — `src/engine/captain.ts:112`.
///
/// Recursive over `multi`. Per variant:
/// - `buffAllies` → a `turn` modifier `` `entry_{Date.now()}` `` (source = the
///   captain's `defId`) on every own board character + log
///   `"Effet d'entree : tous allies +{amount} {STAT} ce tour !"` (stat upper-cased);
/// - `draw` → N `drawCard` + `"Effet d'entree : pioche {n} carte(s)"`;
/// - `damageEnemies` `allFront` → `amount` to V1–V3 (no KO sweep!) +
///   `"Effet d'entree : {n} degats a toute la Ligne Avant ennemie !"`;
/// - `damageEnemies` `single` → strongest-ATK enemy in the front pool (else any),
///   `cursedBonus` replaces `amount` for Cursed targets, `sand` adds 1, log
///   `"Effet d'entree : {n} degats a {name} !"` and the full KO chain
///   (`"{name} est KO !"`); with no enemy character at all the captain takes
///   `amount` with `"Effet d'entree (Gear 2) : {n} degats au Capitaine !"`;
/// - `grantSelfRush` → `captain.deployedTurn = -1` +
///   `"Effet d'entree : Gear 2 — le Capitaine peut agir immédiatement (Rush)."`;
/// - `haoshoku` → every enemy gets a `turn` `-debuffAtk` modifier
///   `` `haoshoku_{slot}_{Date.now()}` `` and, unless `immuneControl`, an
///   `immobilize` 2t when the **printed** `def.def ?? 0 <= immobilizeMaxDef`;
///   log `"Effet d'entree : Haoshoku Haki !"`;
/// - `debuffAllEnemies` → `` `entrydebuff_{slot}_{Date.now()}` `` turn modifier, no log;
/// - `discardOpponentRandom` → N random discards from the opponent's hand
///   ([`EngineContext::rng`]), no log;
/// - `custom` → `"Effet d'entree special : {description}"`.
///
/// `deployedTurn = -1` in TS is a negative turn number; `CaptainInstance.deployed_turn`
/// is `Option<u32>` in Rust, so use `None` (never equal to `state.turn_number`)
/// and document it — see the notes in `state.rs`.
pub fn resolve_entry_effect(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    effect: &EntryEffect,
) -> Result<(), EngineError> {
    todo!("PORT: resolveEntryEffect")
}

/// TS `declareCaptainBaseAttack(state, playerId, targetInstanceId, targetIsCaptain)`
/// — `src/engine/captain.ts:277`.
///
/// Verso-only. Taps the captain, sets both action flags, and parks a
/// [`crate::state::PendingAttack`] whose `attackerId` is the synthetic
/// `` `captain_{playerId}` `` ([`crate::combat::captain_attacker_id`]) with
/// `rawDamage = max(0, versoAtk + atkModifiers - targetDef)`,
/// `hasHaki = verso.naturalHaki non-empty || turnNumber >= 7`. Logs
/// `"Capitaine {name} attaque avec {baseAction.name} ! ({raw} degats)"`.
///
/// Errors: `Captain not flipped (verso required)`, `Captain is tapped`,
/// `Captain base action already used`, `Captain has summoning sickness`.
pub fn declare_captain_base_attack(
    state: &mut GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<(), EngineError> {
    todo!("PORT: declareCaptainBaseAttack")
}

//! Action execution — port of the mutating half of `src/engine/turnManager.ts`
//! (`executeAction` and everything it dispatches to that has no home of its own).
//!
//! The read-only half (`getValidActions`) lives in [`crate::actions`].
//!
//! `executeAction` is the single entry point for every state mutation: it is a
//! pure dispatcher over `GameAction` that forwards to `board.rs`, `combat.rs`,
//! `captain.rs`, `haki.rs`, `fruits.rs` and the local helpers below. A state
//! that already has a `winner` is returned untouched.
#![allow(unused)]

use crate::context::EngineContext;
use crate::error::EngineError;
use crate::registry::CardRegistry;
use crate::state::GameState;
use crate::types::{CounterEffect, EventEffect, GameAction, HakiType, PlayerId, Slot};

// ============================================================
// The dispatcher
// ============================================================

/// TS `executeAction(state, action)` — `src/engine/turnManager.ts:45`.
///
/// Returns the state unchanged when `state.winner` is already set. Otherwise
/// dispatches on `action.type`; every arm uses `state.currentPlayer` as the
/// acting player except `baseAttack` / `specialAttack` / `fruitSpecialAttack`
/// / `playCounter` / `useShield` / `passCounter`, which derive the player from
/// the card or the pending attack. `targetIsCaptain` and `isSpecial` default to
/// `false` when absent.
///
/// Note the TS `captainAttack` arm ignores `action.isSpecial` entirely — it
/// always calls `declareCaptainBaseAttack`. Reproduce that.
///
/// `endTurn` is `endTurn(state)` **followed by** `startTurn(next)`
/// — see [`end_turn_and_start_turn`].
pub fn execute_action(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    action: &GameAction,
) -> Result<(), EngineError> {
    todo!("PORT: executeAction")
}

/// TS `case "endTurn": const next = endTurn(state); return startTurn(next);`
/// — `src/engine/turnManager.ts:122`.
///
/// The two halves themselves are already ported as
/// [`GameState::end_turn`](crate::state::GameState::end_turn) and
/// [`GameState::start_turn`](crate::state::GameState::start_turn) in `state.rs`;
/// this is only the pairing that `executeAction` performs, kept here so
/// `execute.rs` owns the whole action surface.
pub fn end_turn_and_start_turn(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
) -> Result<(), EngineError> {
    state.end_turn(registry)?;
    state.start_turn(registry, ctx)
}

// ============================================================
// Events
// ============================================================

/// TS `playEvent(state, playerId, instanceId)` — `src/engine/turnManager.ts:142`.
///
/// Pays `def.cost`, runs [`resolve_event_effect`] **while the card is still in
/// hand**, then discards it and logs `"Joue {name}"` (so the effect's own logs
/// come first).
///
/// Errors: `Card not found`, `Not your card`, `Card not in hand`,
/// `Not an event card`, `Cannot afford {name}`.
pub fn play_event(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    instance_id: &str,
) -> Result<(), EngineError> {
    todo!("PORT: playEvent")
}

/// TS `resolveEventEffect(state, playerId, effect, cardName)`
/// — `src/engine/turnManager.ts:179`.
///
/// One arm per [`EventEffect`] variant:
/// - `gainWill` → `volonte += amount` (uncapped, no log);
/// - `draw` → N draws, then `discard` oldest-first from the **front** of the
///   hand + `"Defausse automatique de {n} carte(s) (plus ancienne en main)."`;
/// - `healAlly` → only the `allAllies` branch exists (single-target heal is dead
///   code in TS); `min(pv + amount, def.pv)`, ignores `noHeal`;
/// - `buffAllies` → `` `event_{cardName}_{Date.now()}` `` modifier on every own
///   board character, `turn` or `permanent`;
/// - `damageEnemies` → `allFront` restricts to V1–V3, `allCursed` to Cursed;
///   `cursedBonus` **replaces** `amount` for Cursed, `sand` adds 1;
///   `destroyShips` graveyards the enemy ship; then [`sweep_kos`];
/// - `tutor` → first matching character in deck order to hand +
///   `"{cardName} : recherche un personnage."`;
/// - `deployTokens` → N × [`deploy_token`];
/// - `healAllBuff` → heal + `` `feast_{slot}_{Date.now()}` `` turn ATK modifier,
///   skipping units with `noHeal` / `desiccation`;
/// - `debuffAllEnemies` → `` `intim_{slot}_{Date.now()}` `` turn `-atk`, plus an
///   `immobilize` 2t on non-`immuneControl` enemies with effective DEF ≤
///   `immobilizeMaxDef` (quirk: the TS reads the DEF from the *pre-modifier*
///   `next`, not the draft — reproduce that);
/// - `grantHakiAll` → `hakiThisTurn = true`, optional `` `haki_{slot}_{Date.now()}` ``
///   turn ATK modifier, log `"{cardName} : Haki de l'Armement ce tour !"`;
/// - `rally` → `rally_atk_` / `rally_def_` turn modifiers + heal, no log;
/// - `buffSingle` → requires `charKOedThisGame` when `requiresOwnKO`
///   (error `Flashback: aucun de vos personnages n'a été KO ce match`),
///   then `` `flashback_{Date.now()}` `` on the highest effective-ATK ally;
/// - `rushBuff` → `` `burst_{Date.now()}` `` turn ATK on the highest-ATK ally and
///   `deployedTurn = -1` to clear its summoning sickness;
/// - `dodgeAll` → no-op;
/// - `custom` → `execute3` / `execute4` (KO the highest-PV enemy at or below the
///   PV cap, log `"{name} est exécuté !"` for the **opponent**, then
///   [`sweep_kos`]), `coordinatedFire` (damage = number of own `marine`-tagged
///   board characters, dealt to the lowest effective-DEF enemy, log
///   `"Ordre de Tir : {n} dégâts coordonnés."`), everything else just logs
///   `"{cardName} : {description}"`.
pub fn resolve_event_effect(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    effect: &EventEffect,
    card_name: &str,
) -> Result<(), EngineError> {
    todo!("PORT: resolveEventEffect")
}

// ============================================================
// Tokens / KO sweep
// ============================================================

/// TS `deployToken(state, playerId, tokenDefId, slot?)`
/// — `src/engine/turnManager.ts:457`.
///
/// Places a fresh token instance (cost-free, `currentPv = def.pv ?? 1`,
/// `deployedTurn = turnNumber`) into `slot` when it is free, otherwise into the
/// first empty slot in board order; a full board is a silent no-op. Logs
/// `"Déploie {name}."` and then runs `recalculate_passive_buffs(playerId)`.
pub fn deploy_token(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    token_def_id: &str,
    slot: Option<Slot>,
) -> Result<(), EngineError> {
    todo!("PORT: deployToken")
}

/// TS `sweepKOs(state, victimPlayerId, killerPlayerId)`
/// — `src/engine/turnManager.ts:478`.
///
/// Walks `victimPlayerId`'s board in slot order and, for each character at
/// `currentPv <= 0`, logs `"{name} est KO !"` (player = the victim),
/// grants the +2 Vol KO bonus to the victim, removes it from the board and runs
/// `apply_on_ko_effects(victim, killer, koDefId)`.
pub fn sweep_kos(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    victim_player_id: PlayerId,
    killer_player_id: PlayerId,
) -> Result<(), EngineError> {
    todo!("PORT: sweepKOs")
}

// ============================================================
// Counters
// ============================================================

/// TS `playCounter(state, instanceId)` — `src/engine/turnManager.ts:502`.
///
/// Routes on `def.counterEffect.type`: `reduceDamage` →
/// [`crate::combat::apply_counter_reduce`], `survive` →
/// [`crate::combat::apply_counter_survive`], `cancel` / `untargetable` →
/// [`crate::combat::apply_counter_cancel`].
///
/// Errors: `Counter not found`, `Not a counter card`, `No counter effect`.
pub fn play_counter(
    state: &mut GameState,
    registry: &CardRegistry,
    instance_id: &str,
) -> Result<(), EngineError> {
    todo!("PORT: playCounter")
}

// ============================================================
// Ship actives
// ============================================================

/// TS `activateShipAbility(state, playerId, shipInstanceId)`
/// — `src/engine/turnManager.ts:529`.
///
/// Pays `shipActive.cost`, records the 1x/game use, then dispatches — first by
/// ship id, then by lower-cased description sniffing (reproduce the exact
/// substring tests and the `/(\d+)/` style regexes):
/// - `MG-021` Thousand Sunny "Gaon Cannon": N damage (first number in the
///   description, default 5) to the lowest effective-DEF enemy in the front pool
///   (else any), log `"{ship} : Gaon Cannon — {n} dégâts !"` + [`sweep_kos`];
///   with no enemy character it hits the captain
///   (`"{ship} : Gaon Cannon — {n} dégâts au Capitaine !"`) + win check;
/// - `BW-019` Navire Baroque Works: two `TOK-AGENT` tokens +
///   `"{ship} : deux agents déployés !"`;
/// - `RH-017` Red Force: `hakiThisTurn = true` and `+1` turn ATK
///   (`` `redforce_{slot}_{Date.now()}` ``) +
///   `"{ship} : Haki d'Armement et +1 ATK ce tour !"`;
/// - description containing `"deg."` **and** `"avant"`: `/(\d+)\s*deg/` damage
///   to enemy V1–V3 (no KO sweep) + `"{ship} active {active} !"`;
/// - description containing `"+"` and `"atk"`/`"def"`: `/\+(\d+)\s*atk/` and
///   `/\+(\d+)\s*def/` turn buffs on every own board character
///   (`` `ship_{activeName}_atk_{Date.now()}` ``) + `"{ship} active {active} !"`;
/// - fallback: just `"{ship} active {active} !"`.
///
/// Errors: `Ship not found`, `Ship has no active ability`,
/// `Ship ability already used (1x/game)`, `Cannot afford {name} (cost {n})`.
pub fn activate_ship_ability(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    ship_instance_id: &str,
) -> Result<(), EngineError> {
    todo!("PORT: activateShipAbility")
}

// ============================================================
// Support base actions
// ============================================================

/// TS `executeSupportAction(state, playerId, instanceId, targetInstanceId?)`
/// — `src/engine/turnManager.ts:674`.
///
/// Taps the character and burns **both** action flags, then takes the *first*
/// matching branch of `def.baseAction`:
/// `scry` (sort the top N of the deck by ascending printed cost, log
/// `"{name} utilise {ba} : réorganise le dessus du deck."`) →
/// `bluff` (`loseAction` 2t, `"… : {target} a peur et perd sa prochaine action !"`) →
/// `buffAllyAtk` (`` `support_{defId}_{Date.now()}` `` turn ATK,
/// `"… : {target} +{n} ATK ce tour."`) →
/// `immobilize` (`immobilize` 2t, `"… : {target} est immobilise !"`) →
/// a `description` containing `"piege"`/`"Piege"` (permanent `trap` at 3
/// damage/turn, `"… : piege pose sur {target} !"`, error `Trap needs a target`) →
/// `healAmount` (`"… : +{n} PV a {target}"`) →
/// the fallback global `+1` turn ATK on every ally
/// (`` `support_{baName}_{Date.now()}` ``) with `"{name} utilise {ba} !"`.
///
/// Errors: `Card not found`, `Not your card`, `Action already used`,
/// `Not a support action`, `Trap needs a target`.
pub fn execute_support_action(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    instance_id: &str,
    target_instance_id: Option<&str>,
) -> Result<(), EngineError> {
    todo!("PORT: executeSupportAction")
}

// ============================================================
// Haki routing
// ============================================================

/// TS `handleHaki(state, playerId, action)` — `src/engine/turnManager.ts:809`.
///
/// `observation` → [`crate::haki::use_observation_haki`], `king` →
/// [`crate::haki::use_king_haki`], `armament` → no-op (it is a passive from T7).
/// `action.targetInstanceId` is never read by the TS engine.
pub fn handle_haki(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    player_id: PlayerId,
    haki_type: HakiType,
    target_instance_id: Option<&str>,
) -> Result<(), EngineError> {
    todo!("PORT: handleHaki")
}

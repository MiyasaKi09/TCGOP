//! The opponent AI — port of `src/engine/ai.ts`.
//!
//! Three difficulties over one action list:
//! - **beginner** — 80 % chance to just take the hit on defence, then a 35 %
//!   uniform-random pick out of a "basic actions" pool, otherwise the score
//!   heuristic with `Math.random() * 7` jitter;
//! - **intermediate** — the pure score heuristic, no jitter;
//! - **expert** — 1-ply lookahead: apply each action to a clone, run
//!   [`evaluate_state`], add `scoreAction * 0.02` as a tie-break and subtract
//!   `6` from `endTurn`; an action that throws scores `-Infinity`.
//!
//! Every `Math.random()` here goes through [`EngineContext::rng`], and the
//! expert search calls [`crate::execute::execute_action`] on a **clone** of the
//! state (and of the context, so speculative RNG draws do not advance the real
//! stream — see the notes in `lib.rs`).
#![allow(unused)]

use serde::{Deserialize, Serialize};

use crate::context::EngineContext;
use crate::registry::CardRegistry;
use crate::state::GameState;
use crate::types::{GameAction, PlayerId};

/// TS `Difficulty = "beginner" | "intermediate" | "expert"` — `src/engine/ai.ts:12`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Difficulty {
    #[serde(rename = "beginner")]
    Beginner,
    /// TS default parameter of `aiChooseAction`.
    #[default]
    #[serde(rename = "intermediate")]
    Intermediate,
    #[serde(rename = "expert")]
    Expert,
}

/// TS `BASIC` — `src/engine/ai.ts:43`. The action-type discriminators the
/// beginner AI prefers, in set-literal order.
pub const BASIC_ACTION_TYPES: [&str; 7] = [
    "deployCharacter",
    "baseAttack",
    "moveCharacter",
    "endTurn",
    "passCounter",
    "playCounter",
    "useShield",
];

/// TS `aiChooseAction(state, playerId, difficulty = "intermediate")`
/// — `src/engine/ai.ts:20`.
///
/// Falls back to `endTurn` on an empty action list and returns the sole action
/// when there is exactly one, before any difficulty branching.
pub fn ai_choose_action(
    state: &GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    difficulty: Difficulty,
) -> GameAction {
    todo!("PORT: aiChooseAction")
}

/// TS `chooseByScore(state, playerId, actions, jitter)` — `src/engine/ai.ts:33`.
///
/// First action wins ties (`score > bestScore`, strict). `jitter > 0` adds
/// `Math.random() * jitter`.
pub fn choose_by_score(
    state: &GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    actions: &[GameAction],
    jitter: f64,
) -> GameAction {
    todo!("PORT: chooseByScore")
}

/// TS `chooseBeginner(state, playerId, actions)` — `src/engine/ai.ts:45`.
pub fn choose_beginner(
    state: &GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    actions: &[GameAction],
) -> GameAction {
    todo!("PORT: chooseBeginner")
}

/// TS `chooseExpert(state, playerId, actions)` — `src/engine/ai.ts:58`.
pub fn choose_expert(
    state: &GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    actions: &[GameAction],
) -> GameAction {
    todo!("PORT: chooseExpert")
}

/// TS `evaluateState(state, playerId)` — `src/engine/ai.ts:78`.
///
/// `1.5 ×` the captain-PV difference, `±1000` for a dead captain or a decided
/// winner, `atk + def + 0.5 × pv + 3` per own board character (minus the same
/// for the opponent's), `1.4 ×` own hand minus `1.0 ×` enemy hand, `0.4 ×` own
/// Volonté. All arithmetic is `f64` — keep it so the tie-breaks match.
pub fn evaluate_state(state: &GameState, registry: &CardRegistry, player_id: PlayerId) -> f64 {
    todo!("PORT: evaluateState")
}

/// TS `scoreAction(state, playerId, action)` — `src/engine/ai.ts:102`.
///
/// `deployShip` 15, `playCounter` 50, `passCounter` 0, `useHaki` 40 for
/// observation / 60 for king / 10 otherwise, `moveCharacter` 2, `endTurn` −1,
/// anything not listed 0.
pub fn score_action(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    action: &GameAction,
) -> f64 {
    todo!("PORT: scoreAction")
}

/// TS `scoreDeployCharacter(state, playerId, action)` — `src/engine/ai.ts:140`.
///
/// `10 + 2×atk + pv`, `+10` under 3 board characters, `+20` at zero, `+3` when
/// the slot matches `preferredRow`.
pub fn score_deploy_character(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    action: &GameAction,
) -> f64 {
    todo!("PORT: scoreDeployCharacter")
}

/// TS `scoreEquipObject(state, action)` — `src/engine/ai.ts:168`:
/// `8 + 3 × bonusAtk`.
pub fn score_equip_object(
    state: &GameState,
    registry: &CardRegistry,
    action: &GameAction,
) -> f64 {
    todo!("PORT: scoreEquipObject")
}

/// TS `scoreAttack(state, playerId, action)` — `src/engine/ai.ts:178`.
///
/// `5`, `+15` against a captain, and against a character `+20` for a lethal hit
/// plus `max(0, 5 - target.currentPv)`; `+5` more for a `specialAttack`.
/// A `captainAttack` routes here too, and its missing `attackerInstanceId`
/// makes the attacker ATK read as `0` (quirk — reproduce it).
pub fn score_attack(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    action: &GameAction,
) -> f64 {
    todo!("PORT: scoreAttack")
}

/// TS `scoreEvent(state, playerId, action)` — `src/engine/ai.ts:219`.
///
/// `gainWill` 18, `draw` 14, `healAlly` 8, `buffAllies` `12 + 2 × allies`,
/// `damageEnemies` `15 + 2 × amount`, `dodgeAll` 3, any other effect 6, and 5
/// when the card has no `eventEffect` at all.
pub fn score_event(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    action: &GameAction,
) -> f64 {
    todo!("PORT: scoreEvent")
}

/// TS `scoreShield(state, action)` — `src/engine/ai.ts:248`.
///
/// 32 when the block both saves a KO and the blocker survives, 20 when the
/// redirected damage is 0, 6 when it is strictly lower than the original and
/// the blocker survives, else 0.
pub fn score_shield(state: &GameState, registry: &CardRegistry, action: &GameAction) -> f64 {
    todo!("PORT: scoreShield")
}

/// TS `scoreCaptainFlip(state, playerId)` — `src/engine/ai.ts:272`.
///
/// −10 before T4; 30 at 0 allies from T5; 25 at ≤1 ally from T6; 15 from T7;
/// 10 from T5 with ≤2 allies; −5 otherwise.
pub fn score_captain_flip(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> f64 {
    todo!("PORT: scoreCaptainFlip")
}

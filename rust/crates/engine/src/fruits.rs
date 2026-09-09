//! Devil Fruits — port of `src/engine/fruits.ts`.
//!
//! A fruit is an `object` card with `subtype: "fruit"` and structured
//! `fruitEffects`. Equipping it runs [`apply_fruit_base_effects`]; from
//! `awakening.minTurns` on, the legitimate bearer may pay `awakening.volCost`
//! to [`awaken_fruit`], which unlocks the extra traits, stat bonuses and the
//! awakening special attack used by
//! [`crate::combat::declare_fruit_special_attack`].
#![allow(unused)]

use crate::error::EngineError;
use crate::registry::CardRegistry;
use crate::state::GameState;
use crate::types::{PlayerId, Trait};

/// TS `applyFruitBaseEffects(state, fruitInstanceId, bearerInstanceId)`
/// — `src/engine/fruits.ts:12`.
///
/// Pushes the permanent `` `fruit_atk_{fruitInstanceId}` `` / `` `fruit_def_…` ``
/// modifiers (source `` `fruit_{fruitDefId}` ``) — but note the TS **quirk**:
/// both are inside `if (base.grantsTraits)`, so a fruit without granted traits
/// gets no stat bonus at all. Always logs
/// `"{bearerName} mange le {fruitName} ! {base.passiveDescription ?? ""}"`
/// (trailing space kept when the description is absent).
/// A missing fruit instance or a fruit without `fruitEffects` is a silent no-op.
pub fn apply_fruit_base_effects(
    state: &mut GameState,
    registry: &CardRegistry,
    fruit_instance_id: &str,
    bearer_instance_id: &str,
) -> Result<(), EngineError> {
    todo!("PORT: applyFruitBaseEffects")
}

/// TS `canAwakenFruit(state, playerId, fruitInstanceId)`
/// — `src/engine/fruits.ts:65`.
///
/// False when the fruit is missing or already awakened, when it has no
/// `awakening`, when no board character of `playerId` has it attached, when the
/// bearer's name does not `includes(awakening.porteurLegitime)`, when
/// `turnNumber < awakening.minTurns`, or when `volCost` is unaffordable.
///
/// The bearer search is TS `Object.values(state.cards).find(...)` — reproduce it
/// over the `BTreeMap` in key order (deterministic; a fruit can only be attached
/// to one character anyway).
pub fn can_awaken_fruit(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    fruit_instance_id: &str,
) -> Result<bool, EngineError> {
    todo!("PORT: canAwakenFruit")
}

/// TS `awakenFruit(state, playerId, fruitInstanceId)` — `src/engine/fruits.ts:102`.
///
/// Pays `awakening.volCost`, sets `isAwakened`, pushes the permanent
/// `` `fruit_awaken_atk_{fruitInstanceId}` `` / `` `fruit_awaken_def_…` ``
/// modifiers (source `` `fruit_awaken_{fruitDefId}` ``), logs
/// `"⭐ EVEIL ! {bearerName} eveille le {fruitName} ! {awakening.passiveDescription ?? ""}"`
/// and finally calls `recalculate_passive_buffs(playerId)`.
///
/// Errors: `Cannot awaken this fruit`, `No bearer found`.
pub fn awaken_fruit(
    state: &mut GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    fruit_instance_id: &str,
) -> Result<(), EngineError> {
    todo!("PORT: awakenFruit")
}

/// TS `getFruitTraits(state, instanceId)` — `src/engine/fruits.ts:164`.
///
/// The traits granted by the character's equipped fruits, in
/// `attachedObjects` order: `base.grantsTraits` always, plus
/// `awakening.grantsTraits` once the fruit is awakened. Duplicates are kept,
/// exactly like the TS `push(...)`.
pub fn get_fruit_traits(
    state: &GameState,
    registry: &CardRegistry,
    instance_id: &str,
) -> Result<Vec<Trait>, EngineError> {
    todo!("PORT: getFruitTraits")
}

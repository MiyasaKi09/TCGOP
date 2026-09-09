//! Haki — port of `src/engine/haki.ts`.
//!
//! Three Haki types unlock on fixed turn thresholds. Observation (T5+) is a
//! once-per-turn dodge played in the counter window; Armament (T7+) is a pure
//! passive in Rulebook v3.1 and is therefore **never** available as an action;
//! King / Roi (T10+) is a once-per-game board wipe of every enemy with
//! effective DEF ≤ 3, gated on controlling a Conquerant unit.
#![allow(unused)]

use crate::context::EngineContext;
use crate::error::EngineError;
use crate::registry::CardRegistry;
use crate::state::GameState;
use crate::types::{HakiType, PlayerId, Trait};

/// TS `HAKI_THRESHOLDS` — `src/engine/haki.ts:199`
/// (`{ observation: 5, armament: 7, king: 10 }`).
pub const HAKI_THRESHOLDS: [(HakiType, u32); 3] = [
    (HakiType::Observation, 5),
    (HakiType::Armament, 7),
    (HakiType::King, 10),
];

/// The turn from which `haki_type` is unlocked (TS `HAKI_THRESHOLDS[hakiType]`).
pub fn haki_threshold(haki_type: HakiType) -> u32 {
    match haki_type {
        HakiType::Observation => 5,
        HakiType::Armament => 7,
        HakiType::King => 10,
    }
}

/// TS `isHakiAvailable(state, playerId, hakiType)` — `src/engine/haki.ts:206`.
///
/// `false` below the turn threshold; then `!observationUsed` for Observation,
/// **always `false`** for Armament (a passive since v3.1), `!kingUsed` for King.
pub fn is_haki_available(state: &GameState, player_id: PlayerId, haki_type: HakiType) -> bool {
    todo!("PORT: isHakiAvailable")
}

/// TS `useObservationHaki(state, playerId)` — `src/engine/haki.ts:226`.
///
/// Sets `observationUsed`, clears `pendingAttack` and logs
/// `"Haki de l'Observation ! Attaque esquivee !"`.
///
/// Errors: `Observation Haki not available`, `No pending attack to dodge`,
/// `This attack cannot be dodged`.
pub fn use_observation_haki(
    state: &mut GameState,
    player_id: PlayerId,
) -> Result<(), EngineError> {
    todo!("PORT: useObservationHaki")
}

/// TS `hasConquerorInPlay(state, playerId)` — `src/engine/haki.ts:248`.
///
/// True when the captain's current face (verso traits when flipped, otherwise
/// the `CaptainDef.traits`) carries `conqueror`, when `CaptainDef.traits` does,
/// or when any board character's def does.
pub fn has_conqueror_in_play(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Result<bool, EngineError> {
    todo!("PORT: hasConquerorInPlay")
}

/// TS `useKingHaki(state, playerId)` — `src/engine/haki.ts:265`.
///
/// Sets `kingUsed`, logs `"👑 Haki des Rois ! Tous les ennemis DEF ≤ 3 sont KO !"`,
/// then snapshots the victims (effective DEF ≤ 3, board order) **before**
/// removing any, and for each logs `"{name} est KO (Haki des Rois) !"`,
/// grants the +2 Vol KO bonus to the victim's owner, removes it and runs
/// `apply_on_ko_effects(victimOwner, playerId, victimDefId)`.
///
/// Errors: `Roi Haki not available`,
/// `Roi Haki requires a Conquerant unit in play`.
pub fn use_king_haki(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    player_id: PlayerId,
) -> Result<(), EngineError> {
    todo!("PORT: useKingHaki")
}

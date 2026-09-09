//! TCGOP rules engine — a pure, deterministic library.
//!
//! Design contract (mirrors the original TypeScript engine 1:1):
//! - `GameState` is plain data (`serde`), fully cloneable.
//! - `valid_actions(&state)` lists every legal `GameAction` for the active player.
//! - `apply(&mut state, action)` mutates the state, appends to the log and to
//!   the announcement queue, and never panics on a legal action.
//! - Randomness is injected (seeded RNG) so games are reproducible.
//!
//! Module map (TS source → Rust module):
//! - `src/types/index.ts`            → [`types`] (definitions, effects, actions)
//!   and [`state`] (runtime instances / `GameState`)
//! - `src/engine/gameState.ts`,
//!   `src/engine/init.ts`,
//!   `src/engine/utils.ts`,
//!   `src/engine/volonte.ts`         → [`state`]
//! - `src/engine/board.ts`           → [`board`] (queries + `removeFromBoard`)
//! - `src/engine/passives.ts`        → [`passives`]
//! - `src/engine/cardRegistry.ts`,
//!   `src/data/cards/index.ts`       → [`registry`]
//! - `src/data/decks.ts`             → [`decks`]
//! - `src/engine/combat.ts`          → [`combat`]
//! - `src/engine/turnManager.ts`     → [`actions`] (`getValidActions`) and
//!   [`execute`] (`executeAction` and everything it dispatches to)
//! - `src/engine/captain.ts`         → [`captain`]
//! - `src/engine/haki.ts`            → [`haki`]
//! - `src/engine/fruits.ts`          → [`fruits`]
//! - `src/engine/ai.ts`              → [`ai`]
//! - `src/data/cards/*`              → [`cards`]
//! - `Math.random()` / `shuffle`     → [`rng`]
//! - module globals (`Math.random`,
//!   `instanceCounter`, `Date.now`)  → [`context`]
//! - `throw new Error(...)`          → [`error`]

pub mod actions;
pub mod ai;
pub mod board;
pub mod captain;
pub mod cards;
pub mod combat;
pub mod context;
pub mod decks;
pub mod error;
pub mod execute;
pub mod fruits;
pub mod haki;
pub mod passives;
pub mod registry;
pub mod rng;
pub mod state;
pub mod types;

#[cfg(test)]
mod tests;

/// Everything a later module needs, in one import.
pub mod prelude {
    pub use crate::actions::{get_valid_actions, try_get_valid_actions};
    pub use crate::ai::{
        BASIC_ACTION_TYPES, Difficulty, ai_choose_action, evaluate_state, score_action,
    };
    pub use crate::board::{
        ValidTarget, ValidTargets, deploy_character, deploy_cost, deploy_ship, equip_object,
        get_adjacent_slots, get_board_characters, get_character_in_slot, get_effective_atk,
        get_effective_def, get_empty_slots, get_slot_of, get_valid_targets, has_front_row,
        has_summoning_sickness, has_trait, is_back_slot, is_front_slot, move_character,
        remove_from_board,
    };
    pub use crate::captain::{
        can_flip_captain, declare_captain_base_attack, flip_captain, resolve_entry_effect,
    };
    pub use crate::cards::{all_cards, all_captains, all_sets, registry as card_registry};
    pub use crate::combat::{
        CAPTAIN_ATTACKER_PREFIX, CounterOption, apply_captain_damage, apply_character_damage,
        apply_counter_cancel, apply_counter_reduce, apply_counter_survive, apply_element_effects,
        apply_element_to_captain, apply_shield_block, apply_spread_hit, attacker_no_dodge,
        attacker_strips_stealth, captain_attacker_id, conditional_atk_bonus, declare_base_attack,
        declare_fruit_special_attack, declare_special_attack, get_attacker_owner,
        get_eligible_counters, resolve_attack, set_survive_played, survive_played,
    };
    pub use crate::context::{EngineContext, generate_instance_id, to_base36};
    pub use crate::execute::{
        activate_ship_ability, deploy_token, end_turn_and_start_turn, execute_action,
        execute_support_action, handle_haki, play_counter, play_event, resolve_event_effect,
        sweep_kos,
    };
    pub use crate::fruits::{
        apply_fruit_base_effects, awaken_fruit, can_awaken_fruit, get_fruit_traits,
    };
    pub use crate::haki::{
        HAKI_THRESHOLDS, haki_threshold, has_conqueror_in_play, is_haki_available,
        use_king_haki, use_observation_haki,
    };
    pub use crate::decks::{
        DECK_SIZE, all_decks, baroque_deck, deck_size, marines_deck, mugiwara_deck, redhair_deck,
        verify_deck, verify_deck_against, verify_decks,
    };
    pub use crate::error::{EngineError, EngineResult};
    pub use crate::passives::{
        apply_enemy_debuff_auras, apply_on_ko_effects, apply_start_of_turn_passives,
        matches_filter, recalculate_passive_buffs,
    };
    pub use crate::registry::CardRegistry;
    pub use crate::rng::{EngineRng, draw_top, draw_top_n};
    pub use crate::state::{
        ALLY_KO_BONUS_VOL, Board, CaptainInstance, CardInstance, GameState, HAND_LIMIT, LogEntry,
        PendingAttack, PlayerState, Players, STARTING_HAND_SIZE, VOLONTE_CAP, create_game,
        create_initial_state, create_player_state,
    };
    pub use crate::types::*;
}

// ============================================================
// Top-level facade
// ============================================================

use crate::context::EngineContext;
use crate::error::EngineResult;
use crate::registry::CardRegistry;
use crate::state::GameState;
use crate::types::{GameAction, PlayerId};

/// Every legal `GameAction` for `state.current_player` — delegates to
/// [`actions::get_valid_actions`] (TS `getValidActions(state, state.currentPlayer)`).
///
/// During a counter window the actions belong to the **defender**, so this
/// facade asks for `state.current_player` first and falls back to the opponent
/// when an attack is pending, exactly like the UI does.
pub fn valid_actions(state: &GameState, registry: &CardRegistry) -> Vec<GameAction> {
    let player = if state.pending_attack.is_some() {
        state.current_player.opponent()
    } else {
        state.current_player
    };
    actions::get_valid_actions(state, registry, player)
}

/// Every legal `GameAction` for an explicit player (TS
/// `getValidActions(state, playerId)`).
pub fn valid_actions_for(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Vec<GameAction> {
    actions::get_valid_actions(state, registry, player_id)
}

/// Apply one action — TS `executeAction(state, action)`.
///
/// **Signature note (see the port notes):** `executeAction` reaches for
/// `Math.random()` (Robin's entry discard, Shanks' `discardOpponentRandom`) and
/// `Date.now()` (modifier ids), which the Rust engine keeps in an explicit
/// [`EngineContext`] rather than in `GameState`. This context-free wrapper
/// therefore cannot be implemented without deciding where that context lives;
/// hosts that already own one should call [`apply_with_ctx`] instead.
#[allow(unused_variables)]
pub fn apply(
    state: &mut GameState,
    registry: &CardRegistry,
    action: GameAction,
) -> EngineResult<()> {
    todo!("PORT: executeAction — context-free wrapper, see apply_with_ctx")
}

/// Apply one action with an explicit [`EngineContext`] — the faithful
/// `executeAction(state, action)` entry point.
pub fn apply_with_ctx(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    action: GameAction,
) -> EngineResult<()> {
    execute::execute_action(state, registry, ctx, &action)
}

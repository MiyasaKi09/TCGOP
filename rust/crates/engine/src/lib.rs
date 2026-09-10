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
        ShipScope, ValidTarget, ValidTargets, can_move, captain_occupies, deploy_character,
        deploy_cost, deploy_ship, equip_object, equip_restriction_ok, get_adjacent_slots,
        get_board_characters, get_character_in_slot, get_effective_atk, get_effective_def,
        get_empty_slots, get_slot_of, get_valid_targets, granted_attack_traits,
        has_free_object_slot, has_front_row, has_summoning_sickness, has_trait, heal_unit,
        is_back_slot, is_front_slot, is_slot_free, max_object_slots, max_pv_of, move_character,
        remove_from_board, ship_passive_scope,
    };
    pub use crate::captain::{
        FreeFlipReason, can_flip_captain, captain_cannot_act, declare_captain_base_attack,
        declare_captain_special_attack, flip_captain, free_flip_reason, resolve_entry_effect,
        use_captain_surcharge,
    };
    pub use crate::cards::{all_captains, all_cards, all_sets, registry as card_registry};
    pub use crate::combat::{
        CAPTAIN_ATTACKER_PREFIX, CounterOption, apply_captain_damage, apply_character_damage,
        apply_counter_cancel, apply_counter_reduce, apply_counter_survive, apply_element_effects,
        apply_element_to_captain, apply_shield_block, apply_spread_hit, attacker_no_dodge,
        attacker_strips_stealth, captain_attacker_id, conditional_atk_bonus, declare_base_attack,
        declare_fruit_special_attack, declare_special_attack, get_attacker_owner,
        get_eligible_counters, resolve_attack, set_survive_played, survive_played,
    };
    pub use crate::context::{EngineContext, generate_instance_id, to_base36};
    pub use crate::decks::{
        DECK_SIZE, all_decks, baroque_deck, deck_size, marines_deck, mugiwara_deck, redhair_deck,
        verify_deck, verify_deck_against, verify_decks,
    };
    pub use crate::error::{EngineError, EngineResult};
    pub use crate::execute::{
        activate_ship_ability, deploy_token, end_turn_and_start_turn, execute_action,
        execute_support_action, handle_haki, play_counter, play_event, resolve_event_effect,
        sweep_kos,
    };
    pub use crate::fruits::{
        apply_fruit_base_effects, awaken_fruit, can_awaken_fruit, get_fruit_traits,
    };
    pub use crate::haki::{
        HAKI_THRESHOLDS, haki_threshold, has_conqueror_in_play, is_haki_available, use_king_haki,
        use_observation_haki,
    };
    pub use crate::passives::{
        apply_enemy_debuff_auras, apply_on_ko_effects, apply_start_of_turn_passives,
        matches_filter, recalculate_passive_buffs,
    };
    pub use crate::registry::CardRegistry;
    pub use crate::rng::{EngineRng, draw_top, draw_top_n};
    pub use crate::state::{
        ALLY_KO_BONUS_VOL, Board, CaptainInstance, CardInstance, GameState, HAND_LIMIT, LogEntry,
        ONCE_STRAWHAT, ONCE_SURCHARGE_PREFIX, ONCE_SURVIVED, PendingAttack, PlayerState, Players,
        STARTING_HAND_SIZE, VOLONTE_CAP, create_game, create_initial_state, create_player_state,
        once_surcharge, transactional,
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
pub fn valid_actions(state: &GameState, registry: &CardRegistry) -> EngineResult<Vec<GameAction>> {
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
) -> EngineResult<Vec<GameAction>> {
    actions::get_valid_actions(state, registry, player_id)
}

/// Apply one action — TS `executeAction(state, action)`.
///
/// **Where the ambient context comes from.** `executeAction` reaches for
/// `Math.random()` (Robin's entry discard, Shanks' `discardOpponentRandom`),
/// for the module-global `instanceCounter` (token instance ids) and for
/// `Date.now()` (modifier ids). None of the three lives in `GameState`, so a
/// context-free entry point has to produce them from the only thing it is
/// given. This wrapper therefore *derives* an [`EngineContext`] from the state
/// itself — see [`derive_ctx`] — which keeps the engine a pure function of
/// `(state, action)` and keeps whole games reproducible from their seed.
///
/// Hosts that already own a long-lived context (and want one continuous random
/// stream across the whole game) should call [`apply_with_ctx`] instead; that
/// is the faithful `executeAction` entry point.
pub fn apply(
    state: &mut GameState,
    registry: &CardRegistry,
    action: GameAction,
) -> EngineResult<()> {
    let mut ctx = derive_ctx(state);
    apply_with_ctx(state, registry, &mut ctx, action)
}

/// The ambient [`EngineContext`] a context-free [`apply`] call uses, derived
/// from `state` alone:
///
/// - `rng` is seeded with a fingerprint of the state (see
///   [`state_fingerprint`]), so the same state always makes the same "random"
///   choice and a replay of the same action sequence reproduces byte-for-byte;
/// - `instance_counter` starts at `state.cards.len()`. Instances are only ever
///   *added* to `GameState::cards` (nothing removes them — a KO'd card moves to
///   the discard zone, it stays in the map), and every
///   `generate_instance_id` call in the engine is immediately followed by the
///   matching insert, so the counter is strictly increasing across calls and
///   generated ids can never collide;
/// - `now_ms` is `state.log.len()`, a monotone stand-in for `Date.now()`: it
///   only feeds modifier / instance id suffixes, and like the TS clock it is
///   constant within a single `executeAction` call.
pub fn derive_ctx(state: &GameState) -> EngineContext {
    let mut ctx = EngineContext::new(state_fingerprint(state), state.log.len() as u64);
    ctx.instance_counter = state.cards.len() as u64;
    ctx
}

/// A cheap FNV-1a fingerprint over the parts of a
/// `GameState` that change as a game progresses. Used only to seed the derived
/// RNG of [`apply`]; it is never compared, stored or serialised.
pub fn state_fingerprint(state: &GameState) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    fn mix(h: &mut u64, bytes: &[u8]) {
        for b in bytes {
            *h ^= u64::from(*b);
            *h = h.wrapping_mul(FNV_PRIME);
        }
    }
    fn mix_u64(h: &mut u64, v: u64) {
        mix(h, &v.to_le_bytes());
    }

    let mut h = FNV_OFFSET;
    mix_u64(&mut h, u64::from(state.turn_number));
    mix_u64(&mut h, player_tag(state.current_player));
    mix_u64(&mut h, player_tag(state.first_player));
    mix_u64(
        &mut h,
        match state.phase {
            types::Phase::Untap => 0,
            types::Phase::Draw => 1,
            types::Phase::WillGain => 2,
            types::Phase::Main => 3,
            types::Phase::End => 4,
        },
    );
    mix_u64(&mut h, state.winner.map_or(2, player_tag));
    mix_u64(&mut h, state.cards.len() as u64);
    mix_u64(&mut h, state.log.len() as u64);
    mix_u64(&mut h, u64::from(state.pending_attack.is_some()));
    if let Some(pa) = &state.pending_attack {
        mix(&mut h, pa.attacker_id.as_bytes());
        mix(&mut h, pa.target_id.as_bytes());
        mix_u64(&mut h, pa.raw_damage as i64 as u64);
    }
    // The tail of the log is what actually moves between two consecutive
    // actions inside one turn, so it carries the entropy.
    for entry in state.log.iter().rev().take(4) {
        mix_u64(&mut h, u64::from(entry.turn));
        mix_u64(&mut h, player_tag(entry.player));
        mix(&mut h, entry.message.as_bytes());
    }
    for id in [PlayerId::Player1, PlayerId::Player2] {
        let p = state.players.get(id);
        mix_u64(&mut h, p.volonte as i64 as u64);
        mix_u64(&mut h, p.hand.len() as u64);
        mix_u64(&mut h, p.deck.len() as u64);
        mix_u64(&mut h, p.graveyard.len() as u64);
        mix_u64(&mut h, p.board.count() as u64);
        mix_u64(&mut h, p.captain.current_pv as i64 as u64);
        mix_u64(&mut h, u64::from(p.captain.flipped));
    }
    h
}

fn player_tag(id: PlayerId) -> u64 {
    match id {
        PlayerId::Player1 => 0,
        PlayerId::Player2 => 1,
    }
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

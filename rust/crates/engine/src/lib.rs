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
//! - `Math.random()` / `shuffle`     → [`rng`]
//! - module globals (`Math.random`,
//!   `instanceCounter`, `Date.now`)  → [`context`]
//! - `throw new Error(...)`          → [`error`]
//!
//! // PORT: the board.ts mutations (deploy / equip / ship / move,
//! // getValidTargets), combat.ts, captain.ts, haki.ts, fruits.ts,
//! // turnManager.ts (executeAction / getValidActions), ai.ts and the card
//! // data sets are later modules built on top of this foundation.

pub mod board;
pub mod context;
pub mod decks;
pub mod error;
pub mod passives;
pub mod registry;
pub mod rng;
pub mod state;
pub mod types;

#[cfg(test)]
mod tests;

/// Everything a later module needs, in one import.
pub mod prelude {
    pub use crate::board::{
        deploy_cost, get_adjacent_slots, get_board_characters, get_character_in_slot,
        get_effective_atk, get_effective_def, get_empty_slots, get_slot_of, has_front_row,
        has_summoning_sickness, has_trait, is_back_slot, is_front_slot, remove_from_board,
    };
    pub use crate::context::{EngineContext, generate_instance_id, to_base36};
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

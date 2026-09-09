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
//!   `src/engine/utils.ts`,
//!   `src/engine/volonte.ts`         → [`state`]
//! - `src/engine/cardRegistry.ts`,
//!   `src/data/cards/index.ts`       → [`registry`]
//! - `src/data/decks.ts`             → [`decks`]
//! - `Math.random()` / `shuffle`     → [`rng`]
//! - `throw new Error(...)`          → [`error`]
//!
//! // PORT: board.ts, combat.ts, captain.ts, haki.ts, fruits.ts, passives.ts,
//! // turnManager.ts (executeAction / getValidActions), ai.ts and the card
//! // data sets are later modules built on top of this foundation.

pub mod decks;
pub mod error;
pub mod registry;
pub mod rng;
pub mod state;
pub mod types;

#[cfg(test)]
mod tests;

/// Everything a later module needs, in one import.
pub mod prelude {
    pub use crate::decks::{
        DECK_SIZE, all_decks, baroque_deck, deck_size, marines_deck, mugiwara_deck, redhair_deck,
        verify_deck, verify_deck_against,
    };
    pub use crate::error::{EngineError, EngineResult};
    pub use crate::registry::CardRegistry;
    pub use crate::rng::{EngineRng, draw_top, draw_top_n};
    pub use crate::state::{
        ALLY_KO_BONUS_VOL, Board, CaptainInstance, CardInstance, GameState, HAND_LIMIT, LogEntry,
        PendingAttack, PlayerState, Players, STARTING_HAND_SIZE, VOLONTE_CAP, create_initial_state,
        create_player_state, generate_instance_id,
    };
    pub use crate::types::*;
}

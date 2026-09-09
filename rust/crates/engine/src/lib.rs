//! TCGOP rules engine — a pure, deterministic library.
//!
//! Design contract (mirrors the original TypeScript engine 1:1):
//! - `GameState` is plain data (`serde`), fully cloneable.
//! - `valid_actions(&state)` lists every legal `GameAction` for the active player.
//! - `apply(&mut state, action)` mutates the state, appends to the log and to
//!   the announcement queue, and never panics on a legal action.
//! - Randomness is injected (seeded RNG) so games are reproducible.
//!
//! Modules are filled in by the port; this file only fixes the public surface.

pub mod prelude {
    // Re-exported once the modules exist.
}

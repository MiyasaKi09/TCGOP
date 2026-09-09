//! Token bodies deployed by effects (never part of a deck).
//!
//! Port of `src/data/cards/tokens.ts` (`tokenCards`) — 3 card
//! definitions, in the exact source order (the order is load-bearing: the
//! registry is filled by iterating it, and later ids overwrite earlier ones).
#![allow(unused)]

use crate::types::CardDef;

/// TS `tokenCards: CardDef[]` (`src/data/cards/tokens.ts`).
///
/// Returns the 3 definitions of the TOKEN set in source order.
pub fn cards() -> Vec<CardDef> {
    todo!("PORT: tokenCards")
}

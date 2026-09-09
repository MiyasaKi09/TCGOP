//! ST02 — Marines card data.
//!
//! Port of `src/data/cards/marines.ts` (`marinesCards`) — 28 card
//! definitions, in the exact source order (the order is load-bearing: the
//! registry is filled by iterating it, and later ids overwrite earlier ones).
#![allow(unused)]

use crate::types::CardDef;

/// TS `marinesCards: CardDef[]` (`src/data/cards/marines.ts`).
///
/// Returns the 28 definitions of the ST02 — Marines set in source order.
pub fn cards() -> Vec<CardDef> {
    todo!("PORT: marinesCards")
}

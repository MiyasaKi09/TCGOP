//! ST03 — Baroque Works card data.
//!
//! Port of `src/data/cards/baroque.ts` (`baroqueCards`) — 28 card
//! definitions, in the exact source order (the order is load-bearing: the
//! registry is filled by iterating it, and later ids overwrite earlier ones).
#![allow(unused)]

use crate::types::CardDef;

/// TS `baroqueCards: CardDef[]` (`src/data/cards/baroque.ts`).
///
/// Returns the 28 definitions of the ST03 — Baroque Works set in source order.
pub fn cards() -> Vec<CardDef> {
    todo!("PORT: baroqueCards")
}

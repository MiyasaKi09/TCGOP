//! ST04 — équipage de Shanks le Roux card data.
//!
//! Port of `src/data/cards/redhair.ts` (`redhairCards`) — 27 card
//! definitions, in the exact source order (the order is load-bearing: the
//! registry is filled by iterating it, and later ids overwrite earlier ones).
#![allow(unused)]

use crate::types::CardDef;

/// TS `redhairCards: CardDef[]` (`src/data/cards/redhair.ts`).
///
/// Returns the 27 definitions of the ST04 — Red Hair Pirates set in source order.
pub fn cards() -> Vec<CardDef> {
    todo!("PORT: redhairCards")
}

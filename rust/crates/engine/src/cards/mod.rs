//! The card catalogue — port of `src/data/cards/`.
//!
//! `src/data/cards/index.ts` concatenates the five card sets into `allCards`
//! and the four captains into `allCaptains`, then folds them into the two
//! module-global registries. The Rust port keeps the data in per-set functions
//! and builds an explicit [`CardRegistry`] from them; the **aggregation order is
//! part of the contract** because a duplicate id would resolve to the last
//! registration, exactly as in the TS `for (const card of allCards)` loop.

pub mod baroque;
pub mod captains;
pub mod marines;
pub mod mugiwara;
pub mod redhair;
pub mod tokens;

use crate::registry::CardRegistry;
use crate::types::{CaptainDef, CardDef};

/// TS `allCards = [...mugiwaraCards, ...marinesCards, ...baroqueCards,
/// ...redhairCards, ...tokenCards]` (`src/data/cards/index.ts:9`), kept as one
/// `Vec` per set so the aggregation order stays explicit.
pub fn all_sets() -> Vec<Vec<CardDef>> {
    vec![
        mugiwara::cards(),
        marines::cards(),
        baroque::cards(),
        redhair::cards(),
        tokens::cards(),
    ]
}

/// TS `allCards` flattened — the 114 definitions in `index.ts` order.
pub fn all_cards() -> Vec<CardDef> {
    all_sets().into_iter().flatten().collect()
}

/// TS `allCaptains` (re-exported from `captains.ts` by `index.ts`).
pub fn all_captains() -> Vec<CaptainDef> {
    captains::captains()
}

/// TS `cardRegistry` + `captainRegistry` (`src/data/cards/index.ts:17,22`),
/// i.e. the registry `init.ts::initializeRegistry()` builds at boot.
pub fn registry() -> CardRegistry {
    CardRegistry::from_sets(all_sets(), all_captains())
}

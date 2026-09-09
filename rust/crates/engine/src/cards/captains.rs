//! The four captain definitions.
//!
//! Port of `src/data/cards/captains.ts` — `captainLuffy` (`CAP-LUFFY`),
//! `captainAkainu` (`CAP-AKAINU`), `captainCrocodile` (`CAP-CROCODILE`) and
//! `captainShanks` (`CAP-SHANKS`), aggregated by `allCaptains` in that order.
#![allow(unused)]

use crate::types::CaptainDef;

/// TS `captainLuffy` (`src/data/cards/captains.ts:3`) — `CAP-LUFFY`.
pub fn captain_luffy() -> CaptainDef {
    todo!("PORT: captainLuffy")
}

/// TS `captainAkainu` (`src/data/cards/captains.ts:55`) — `CAP-AKAINU`.
pub fn captain_akainu() -> CaptainDef {
    todo!("PORT: captainAkainu")
}

/// TS `captainCrocodile` (`src/data/cards/captains.ts:93`) — `CAP-CROCODILE`.
pub fn captain_crocodile() -> CaptainDef {
    todo!("PORT: captainCrocodile")
}

/// TS `captainShanks` (`src/data/cards/captains.ts:129`) — `CAP-SHANKS`.
pub fn captain_shanks() -> CaptainDef {
    todo!("PORT: captainShanks")
}

/// TS `allCaptains: CaptainDef[] = [captainLuffy, captainAkainu,
/// captainCrocodile, captainShanks]` (`src/data/cards/captains.ts:166`).
///
/// The order is load-bearing — `CardRegistry::register_captains` iterates it.
pub fn captains() -> Vec<CaptainDef> {
    vec![
        captain_luffy(),
        captain_akainu(),
        captain_crocodile(),
        captain_shanks(),
    ]
}

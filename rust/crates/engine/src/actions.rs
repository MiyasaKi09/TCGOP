//! Legal-action enumeration — port of `getValidActions`
//! (`src/engine/turnManager.ts:829`).
//!
//! The generation order is **load-bearing**: the AI's tie-breaks and every
//! recorded replay depend on it. In the counter window only the defender has
//! actions; otherwise the list is built for the current player during the
//! `main` phase in exactly this sequence:
//!
//! 1. `deployCharacter` — hand order × empty slots in board order
//! 2. `deployShip` — hand order
//! 3. `equipObject` — hand order × own board characters in slot order
//! 4. `playEvent` — hand order
//! 5. `baseSupportAction` — own board characters in slot order
//! 6. `baseAttack` — characters × character targets, then the captain
//! 7. `specialAttack` — same shape
//! 8. `fruitSpecialAttack` — characters × attached awakened fruits
//! 9. `flipCaptain` — one per empty slot
//! 10. `captainAttack` — enemy captain first, then enemy characters
//! 11. `moveCharacter` — characters × adjacent empty slots
//! 12. `activateShip`
//! 13. `awakenFruit`
//! 14. `useHaki { king }`
//! 15. `endTurn` (always last, always present)
#![allow(unused)]

use crate::error::EngineError;
use crate::registry::CardRegistry;
use crate::state::GameState;
use crate::types::{GameAction, PlayerId};

/// TS `getValidActions(state, playerId)` — `src/engine/turnManager.ts:829`.
///
/// **Counter window** (`state.pendingAttack` set): returns nothing unless
/// `playerId` is `getOpponent(state.currentPlayer)`; then the eligible counters
/// ([`crate::combat::get_eligible_counters`], hand order), `passCounter`, the
/// Observation dodge when available and the attack is dodgeable, and one
/// `useShield` per untapped adjacent Bouclier ally (adjacency of the target's
/// slot, or of the captain's slot when the captain is the target).
///
/// **Main phase**: empty unless `playerId == state.currentPlayer` and
/// `state.phase == "main"`, then the 15 groups listed in the module docs.
/// Gating details that must be reproduced exactly:
/// - support / base attacks skip tapped, `usedBaseAction`, summoning-sick and
///   `freeze` / `immobilize` / `sleep` / `loseAction` characters; base attacks
///   additionally require effective ATK > 0 and honour `cannotAttackFemale`;
/// - specials skip `usedSpecialAttack`, once-per-game exhaustion, unaffordable
///   costs and `isSupport` specials, but a `transform` special is offered with
///   the attacker itself as the target;
/// - fruit specials only skip `freeze` / `immobilize` (not `sleep` /
///   `loseAction` — quirk);
/// - the captain may attack when flipped, on-board, untapped, not
///   frozen/immobilised, and either not deployed this turn or `rush`;
/// - `useHaki { king }` needs [`crate::haki::is_haki_available`],
///   [`crate::haki::has_conqueror_in_play`] and at least one enemy at
///   effective DEF ≤ 3.
///
/// Infallible, unlike the TS (which would `throw` from `getCardDef`): a state
/// whose card ids are all in `registry` behaves identically, and an unknown
/// def id simply contributes no actions.
pub fn get_valid_actions(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Vec<GameAction> {
    todo!("PORT: getValidActions")
}

/// Fallible twin of [`get_valid_actions`] for callers that want the TS
/// `getCardDef` throw surfaced instead of swallowed.
pub fn try_get_valid_actions(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Result<Vec<GameAction>, EngineError> {
    todo!("PORT: getValidActions (fallible)")
}

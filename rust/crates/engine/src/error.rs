//! Engine error type.
//!
//! The TypeScript engine signals rule violations with `throw new Error("...")`.
//! Every such throw maps to one of these variants; the `Display` strings keep
//! the original TS messages where one exists so logs stay comparable.

use thiserror::Error;

use crate::types::{Phase, Slot};

/// All failures the rules engine can report. Never panics on a legal action.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineError {
    /// TS `Card not found: ${id}` (cardRegistry.getCardDef).
    #[error("Card not found: {0}")]
    UnknownCard(String),

    /// TS `Captain not found: ${id}` (cardRegistry.getCaptainDef).
    #[error("Captain not found: {0}")]
    UnknownCaptain(String),

    /// A runtime instance id that is not present in `GameState.cards`
    /// (TS `Card not found`, `Attacker not found`, `Target not found`, …).
    #[error("Instance not found: {0}")]
    UnknownInstance(String),

    /// TS `Not your card` / `Not your character`.
    #[error("Not your card: {0}")]
    NotYourCard(String),

    /// TS `Card not in hand` / `Target not on board` / `Card not on board`.
    #[error("{instance_id} is not in zone {expected}")]
    WrongZone {
        instance_id: String,
        expected: String,
    },

    /// TS `Not a character card` / `Not an object card` / `Not a ship card` / …
    #[error("Wrong card type: expected {expected}")]
    WrongCardType { expected: String },

    /// TS `Slot ${slot} is occupied`.
    #[error("Slot {0:?} is occupied")]
    SlotOccupied(Slot),

    /// TS `${targetSlot} is not adjacent to ${currentSlot}`.
    #[error("{to:?} is not adjacent to {from:?}")]
    NotAdjacent { from: Slot, to: Slot },

    /// TS `Cannot afford ${name} (cost ${cost})` (board.ts deploy / equip / ship).
    #[error("Not enough {what}: has {has}, needs {cost}")]
    CannotAfford { what: String, cost: i32, has: i32 },

    /// TS volonte.ts `spendVolonte`: `Not enough Volonte: has ${player.volonte}, needs ${amount}`.
    #[error("Not enough Volonte: has {has}, needs {needs}")]
    NotEnoughVolonte { has: i32, needs: i32 },

    /// The action is not allowed in the current phase.
    #[error("Wrong phase: expected {expected:?}, current {actual:?}")]
    WrongPhase { expected: Phase, actual: Phase },

    /// TS `No pending attack` / `No pending attack to dodge`.
    #[error("No pending attack")]
    NoPendingAttack,

    /// There is already a pending attack waiting for a counter response.
    #[error("An attack is already pending")]
    AttackPending,

    /// TS `Action already used` / `Special already used` / `Free move already used this turn` / …
    #[error("Already used: {0}")]
    AlreadyUsed(String),

    /// TS `Character has summoning sickness`.
    #[error("Character has summoning sickness")]
    SummoningSickness,

    /// TS `Attacker is tapped` / `Captain is tapped` / `A tapped character cannot use Bouclier`.
    #[error("Tapped: {0}")]
    Tapped(String),

    /// The game already has a winner (`executeAction` returns the state unchanged in TS).
    #[error("Game is over")]
    GameOver,

    /// Rust-only strict deck validation (`decks::verify_deck_against`), with the
    /// text of the TS decks.ts `verifyDeck` warning:
    /// `Deck "${name}" has ${total} cards (expected 50)`.
    #[error("Deck \"{name}\" has {total} cards (expected {expected})")]
    InvalidDeckSize {
        name: String,
        total: u32,
        expected: u32,
    },

    /// A deck references a card id that is not in the registry.
    #[error("Deck \"{deck}\" references unknown card {card_id}")]
    InvalidDeckCard { deck: String, card_id: String },

    /// Any other rule violation, with the exact TS error message.
    #[error("{0}")]
    IllegalAction(String),
}

impl EngineError {
    /// Shorthand for the generic rule-violation variant.
    pub fn illegal(msg: impl Into<String>) -> Self {
        EngineError::IllegalAction(msg.into())
    }
}

/// Convenience alias used across the crate.
pub type EngineResult<T> = Result<T, EngineError>;

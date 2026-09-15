//! Card / captain definition registry — port of `src/engine/cardRegistry.ts`
//! and the aggregation in `src/data/cards/index.ts`.
//!
//! The TS engine keeps a module-global registry (`registerCards`,
//! `registerCaptains`, `getCardDef`, `getCaptainDef`, `getAllCardDefs`,
//! `getAllCaptainDefs`) filled once by `init.ts::initializeRegistry()` with
//! `allCards = [...mugiwaraCards, ...marinesCards, ...baroqueCards,
//! ...redhairCards, ...tokenCards]` and `allCaptains`.
//!
//! The Rust engine has no globals: a `CardRegistry` value is built once and
//! passed by reference to everything that needs definitions.
//!
//! // PORT: the card DATA (src/data/cards/{mugiwara,marines,baroque,redhair,
//! // tokens,captains}.ts) is ported by later agents as functions returning
//! // `Vec<CardDef>` / `Vec<CaptainDef>`; they feed `CardRegistry::from_sets`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::EngineError;
use crate::types::{CaptainDef, CardDef};

/// TS `cardRegistry` + `captainRegistry` (cardRegistry.ts).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CardRegistry {
    cards: HashMap<String, CardDef>,
    captains: HashMap<String, CaptainDef>,
}

impl CardRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// TS `initializeRegistry()` (init.ts) / `allCards` (data/cards/index.ts):
    /// register every set in order, then every captain. Later registrations
    /// of the same id overwrite earlier ones, exactly like the TS
    /// `cardRegistry[card.id] = card` loop.
    pub fn from_sets<S, C>(sets: S, captains: C) -> Self
    where
        S: IntoIterator<Item = Vec<CardDef>>,
        C: IntoIterator<Item = CaptainDef>,
    {
        let mut reg = CardRegistry::new();
        for set in sets {
            reg.register_set(set);
        }
        reg.register_captains(captains);
        reg
    }

    /// TS `registerCards(cards)`.
    pub fn register_set(&mut self, cards: Vec<CardDef>) {
        for card in cards {
            self.register_card(card);
        }
    }

    /// TS `registerCards([card])` — single-card convenience.
    pub fn register_card(&mut self, card: CardDef) {
        self.cards.insert(card.id.clone(), card);
    }

    /// TS `registerCaptains(captains)`.
    pub fn register_captains<C: IntoIterator<Item = CaptainDef>>(&mut self, captains: C) {
        for cap in captains {
            self.register_captain(cap);
        }
    }

    /// TS `registerCaptains([cap])` — single-captain convenience.
    pub fn register_captain(&mut self, captain: CaptainDef) {
        self.captains.insert(captain.id.clone(), captain);
    }

    /// TS `getCardDef(id)` — `Err(UnknownCard)` instead of `throw`.
    pub fn get_card_def(&self, id: &str) -> Result<&CardDef, EngineError> {
        self.cards
            .get(id)
            .ok_or_else(|| EngineError::UnknownCard(id.to_string()))
    }

    /// TS `getCaptainDef(id)` — `Err(UnknownCaptain)` instead of `throw`.
    pub fn get_captain_def(&self, id: &str) -> Result<&CaptainDef, EngineError> {
        self.captains
            .get(id)
            .ok_or_else(|| EngineError::UnknownCaptain(id.to_string()))
    }

    /// Non-failing lookup (TS `cardRegistry[id]` without the throw).
    pub fn card_def(&self, id: &str) -> Option<&CardDef> {
        self.cards.get(id)
    }

    /// Non-failing lookup (TS `captainRegistry[id]` without the throw).
    pub fn captain_def(&self, id: &str) -> Option<&CaptainDef> {
        self.captains.get(id)
    }

    /// TS `getAllCardDefs()` (returned by reference here, no copy).
    pub fn all_card_defs(&self) -> &HashMap<String, CardDef> {
        &self.cards
    }

    /// TS `getAllCaptainDefs()` (returned by reference here, no copy).
    pub fn all_captain_defs(&self) -> &HashMap<String, CaptainDef> {
        &self.captains
    }

    pub fn has_card(&self, id: &str) -> bool {
        self.cards.contains_key(id)
    }

    pub fn has_captain(&self, id: &str) -> bool {
        self.captains.contains_key(id)
    }

    pub fn card_count(&self) -> usize {
        self.cards.len()
    }

    pub fn captain_count(&self) -> usize {
        self.captains.len()
    }
}

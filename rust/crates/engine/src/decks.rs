//! The four pre-built decks — port of `src/data/decks.ts`.
//!
//! Card ids, counts and ordering are copied verbatim from the TS file (the
//! order matters: `createPlayerState` builds instances in deck-list order
//! before shuffling, so the instance ids / counter values depend on it).

use crate::error::EngineError;
use crate::registry::CardRegistry;
use crate::types::{DeckDef, DeckEntry};

/// Legal deck size (decks.ts `verifyDeck`: `expected 50`).
pub const DECK_SIZE: u32 = 50;

fn entry(card_id: &str, count: u32) -> DeckEntry {
    DeckEntry {
        card_id: card_id.to_string(),
        count,
    }
}

/// TS `mugiwaraDeck`.
pub fn mugiwara_deck() -> DeckDef {
    DeckDef {
        name: "Mugiwara - Pre-Ellipse".to_string(),
        captain_id: "CAP-LUFFY".to_string(),
        cards: vec![
            // Personnages (21 cards)
            entry("MG-001", 3), // Zoro
            entry("MG-002", 3), // Sanji
            entry("MG-003", 3), // Nami
            entry("MG-004", 3), // Usopp
            entry("MG-005", 3), // Chopper
            entry("MG-006", 2), // Robin
            entry("MG-007", 2), // Franky
            entry("MG-008", 2), // Brook
            // Armes (5 cards)
            entry("MG-009", 1), // Wado Ichimonji
            entry("MG-010", 1), // Sandai Kitetsu
            entry("MG-011", 1), // Yubashiri
            entry("MG-012", 1), // Clima-Tact
            entry("MG-013", 1), // Kabuto
            // Fruits (3 cards)
            entry("MG-014", 1), // Gomu Gomu
            entry("MG-015", 1), // Hana Hana
            entry("MG-016", 1), // Yomi Yomi
            // Accessoires (3 cards)
            entry("MG-017", 1), // Baril d'Eau
            entry("MG-018", 1), // Dial d'Impact
            entry("MG-019", 1), // Vivre Card
            // Navires (4 cards)
            entry("MG-020", 2), // Going Merry
            entry("MG-021", 2), // Thousand Sunny
            // Evenements (10 cards)
            entry("MG-022", 2), // Volonte du D
            entry("MG-023", 2), // Nakama !
            entry("MG-024", 2), // Flashback
            entry("MG-025", 2), // Tempete
            entry("MG-028", 2), // Coup de Burst
            // Counters (4 cards)
            entry("MG-026", 2), // JE VEUX VIVRE !
            entry("MG-027", 2), // Drapeau Noir
        ],
    }
}

/// TS `marinesDeck`.
pub fn marines_deck() -> DeckDef {
    DeckDef {
        name: "Marines - Justice Absolue".to_string(),
        captain_id: "CAP-AKAINU".to_string(),
        cards: vec![
            // Personnages (21 cards)
            entry("MR-001", 3), // Coby
            entry("MR-002", 2), // Helmeppo
            entry("MR-003", 3), // Tashigi
            entry("MR-004", 2), // Smoker
            entry("MR-005", 2), // Sentomaru
            entry("MR-006", 2), // Momonga
            entry("MR-007", 2), // Garp
            entry("MR-008", 2), // Kizaru
            entry("MR-009", 2), // Aokiji
            entry("MR-010", 1), // Sengoku
            // Fruits (2 cards)
            entry("MR-011", 1), // Magu Magu
            entry("MR-012", 1), // Moku Moku
            // Armes (2 cards)
            entry("MR-013", 1), // Shigure
            entry("MR-014", 1), // Jitte
            // Accessoires (3 cards)
            entry("MR-015", 1), // Menottes
            entry("MR-016", 1), // Canon Marine
            entry("MR-017", 1), // Boulet G. Marin
            // Navires (4 cards)
            entry("MR-018", 2), // Navire de Guerre
            entry("MR-019", 2), // Navire de Justice
            // Evenements (14 cards)
            entry("MR-020", 2), // Buster Call
            entry("MR-021", 2), // Promotion
            entry("MR-022", 2), // Justice Absolue
            entry("MR-023", 2), // Renforts
            entry("MR-024", 2), // Ordre de Tir
            entry("MR-027", 2), // Embargo
            entry("MR-028", 2), // Execution Publique
            // Counters (4 cards)
            entry("MR-025", 2), // Manteau de Justice
            entry("MR-026", 2), // Mur d'Acier
        ],
    }
}

/// TS `baroqueDeck`.
pub fn baroque_deck() -> DeckDef {
    DeckDef {
        name: "Baroque Works - Utopia".to_string(),
        captain_id: "CAP-CROCODILE".to_string(),
        cards: vec![
            entry("BW-001", 3),
            entry("BW-002", 2),
            entry("BW-003", 2),
            entry("BW-004", 2),
            entry("BW-005", 2),
            entry("BW-006", 2),
            entry("BW-007", 2),
            entry("BW-008", 2),
            entry("BW-009", 2),
            entry("BW-010", 2),
            entry("BW-013", 2),
            entry("BW-014", 1),
            entry("BW-011", 1),
            entry("BW-012", 1),
            entry("BW-015", 1),
            entry("BW-016", 1),
            entry("BW-017", 1),
            entry("BW-018", 2),
            entry("BW-019", 2),
            entry("BW-020", 2),
            entry("BW-021", 2),
            entry("BW-022", 2),
            entry("BW-023", 2),
            entry("BW-024", 2),
            entry("BW-027", 2),
            entry("BW-028", 1),
            entry("BW-025", 2),
            entry("BW-026", 2),
        ],
    }
}

/// TS `redhairDeck`.
pub fn redhair_deck() -> DeckDef {
    DeckDef {
        name: "Red Hair - L'Empereur".to_string(),
        captain_id: "CAP-SHANKS".to_string(),
        cards: vec![
            entry("RH-001", 2),
            entry("RH-002", 3),
            entry("RH-003", 2),
            entry("RH-004", 3),
            entry("RH-005", 2),
            entry("RH-006", 3),
            entry("RH-007", 2),
            entry("RH-008", 2),
            entry("RH-009", 2),
            entry("RH-010", 1),
            entry("RH-011", 1),
            entry("RH-012", 1),
            entry("RH-013", 1),
            entry("RH-014", 1),
            entry("RH-015", 1),
            entry("RH-016", 1),
            entry("RH-017", 2),
            entry("RH-018", 2),
            entry("RH-019", 2),
            entry("RH-020", 2),
            entry("RH-021", 2),
            entry("RH-022", 3),
            entry("RH-023", 2),
            entry("RH-026", 2),
            entry("RH-024", 2),
            entry("RH-025", 2),
            entry("RH-027", 1),
        ],
    }
}

/// All four decks in the order they are declared in decks.ts.
pub fn all_decks() -> Vec<DeckDef> {
    vec![
        mugiwara_deck(),
        marines_deck(),
        baroque_deck(),
        redhair_deck(),
    ]
}

/// Total number of cards in a deck (`deck.cards.reduce((sum, e) => sum + e.count, 0)`).
pub fn deck_size(deck: &DeckDef) -> u32 {
    deck.cards.iter().map(|e| e.count).sum()
}

/// TS `verifyDeck(deck)` — the TS version only `console.warn`s; here a wrong
/// size is an `Err(InvalidDeckSize)`.
pub fn verify_deck(deck: &DeckDef) -> Result<(), EngineError> {
    let total = deck_size(deck);
    if total != DECK_SIZE {
        return Err(EngineError::InvalidDeckSize {
            name: deck.name.clone(),
            total,
            expected: DECK_SIZE,
        });
    }
    Ok(())
}

/// `verify_deck` plus a check that every card id and the captain id exist in
/// the registry (the TS engine would only fail later, inside
/// `createPlayerState`, with `Card not found`).
pub fn verify_deck_against(deck: &DeckDef, registry: &CardRegistry) -> Result<(), EngineError> {
    verify_deck(deck)?;
    registry.get_captain_def(&deck.captain_id)?;
    for e in &deck.cards {
        if !registry.has_card(&e.card_id) {
            return Err(EngineError::InvalidDeckCard {
                deck: deck.name.clone(),
                card_id: e.card_id.clone(),
            });
        }
    }
    Ok(())
}

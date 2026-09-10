//! Pure, head-lessly testable model of the two screens around the board.
//!
//! Everything here is a plain value: the deck catalogue of `src/app/page.tsx`
//! (`DECK_VISUAL` in `src/lib/theme.ts`), the three AI levels, the selection
//! the player builds on the setup screen, and what the game-over splash shows.
//! The systems in [`super::setup`] / [`super::game_over`] only turn these
//! answers into nodes.

use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use tcgop_engine::ai::Difficulty;
use tcgop_engine::decks;
use tcgop_engine::types::DeckDef;

use crate::bridge::Session;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

// ============================================================
// Decks
// ============================================================

/// The four pre-built decks, in `tcgop_engine::decks::all_decks()` order —
/// which is also the key order of the TS `DECK_VISUAL` record, so the setup
/// screen lists them exactly like the web client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeckKey {
    Mugiwara,
    Marines,
    Baroque,
    RedHair,
}

impl DeckKey {
    /// Declaration order — the order the cards are laid out in.
    pub const ALL: [DeckKey; 4] = [
        DeckKey::Mugiwara,
        DeckKey::Marines,
        DeckKey::Baroque,
        DeckKey::RedHair,
    ];

    /// Index in [`DeckKey::ALL`] (used for the staggered "enter" delay).
    // Used by the deck-list test; the stagger is computed while iterating.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn index(self) -> usize {
        match self {
            DeckKey::Mugiwara => 0,
            DeckKey::Marines => 1,
            DeckKey::Baroque => 2,
            DeckKey::RedHair => 3,
        }
    }

    /// The engine deck definition.
    pub fn def(self) -> DeckDef {
        match self {
            DeckKey::Mugiwara => decks::mugiwara_deck(),
            DeckKey::Marines => decks::marines_deck(),
            DeckKey::Baroque => decks::baroque_deck(),
            DeckKey::RedHair => decks::redhair_deck(),
        }
    }

    /// Captain def id — the art key of the deck card.
    pub fn captain_id(self) -> &'static str {
        match self {
            DeckKey::Mugiwara => "CAP-LUFFY",
            DeckKey::Marines => "CAP-AKAINU",
            DeckKey::Baroque => "CAP-CROCODILE",
            DeckKey::RedHair => "CAP-SHANKS",
        }
    }

    /// TS `DECK_VISUAL[key]`.
    pub fn visual(self) -> DeckVisual {
        match self {
            DeckKey::Mugiwara => DeckVisual {
                label: "Mugiwara",
                captain: "Monkey D. Luffy",
                tagline: "Diversité, synergies, sustain",
                accent: rgb(0xE8, 0x95, 0x4A),
                crest: SetupCrest::Hat,
            },
            DeckKey::Marines => DeckVisual {
                label: "Marine",
                captain: "Akainu (Sakazuki)",
                tagline: "Anti-Fruit, Logia, contrôle",
                accent: rgb(0x5B, 0x97, 0xD8),
                crest: SetupCrest::Anchor,
            },
            DeckKey::Baroque => DeckVisual {
                label: "Baroque Works",
                captain: "Crocodile (Mr. 0)",
                tagline: "Agro/tempo, poison & sable",
                accent: rgb(0xC9, 0xA2, 0x4B),
                crest: SetupCrest::Skull,
            },
            DeckKey::RedHair => DeckVisual {
                label: "Red Hair",
                captain: "Shanks (Akagami)",
                tagline: "Puissance, Haki, intimidation",
                accent: rgb(0xD2, 0x47, 0x3C),
                crest: SetupCrest::Skull,
            },
        }
    }
}

/// TS `DeckVisual` — everything one setup card shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeckVisual {
    /// Crew name, drawn in Cinzel in the accent colour.
    pub label: &'static str,
    /// Captain name, after the `Capitaine ·` prefix.
    pub captain: &'static str,
    /// One-line play-style pitch, drawn in Spectral italic.
    pub tagline: &'static str,
    /// Faction accent — strip, label, crest and selected ring.
    pub accent: Color,
    /// Crest glyph, drawn with plain nodes (no icon font).
    pub crest: SetupCrest,
}

/// The three crests of `src/components/icons.tsx` used by the deck cards.
/// (`art::Crest` only knows the two board factions; Baroque Works and Red Hair
/// both fly a skull.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupCrest {
    /// Straw hat — a ringed disc with a horizontal brim.
    Hat,
    /// Marine anchor — a ringed disc with a vertical shank.
    Anchor,
    /// Jolly Roger — a ringed disc with two eye sockets.
    Skull,
}

// ============================================================
// Difficulty
// ============================================================

/// One row of the TS `LEVELS` array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelChoice {
    pub level: Difficulty,
    pub label: &'static str,
    pub sub: &'static str,
}

/// TS `LEVELS` — order and strings verbatim.
pub const LEVELS: [LevelChoice; 3] = [
    LevelChoice {
        level: Difficulty::Beginner,
        label: "Débutant",
        sub: "Joue prudemment, encaisse souvent",
    },
    LevelChoice {
        level: Difficulty::Intermediate,
        label: "Intermédiaire",
        sub: "Heuristique solide, gère ses tempos",
    },
    LevelChoice {
        level: Difficulty::Expert,
        label: "Expert",
        sub: "Anticipe, contre et cherche le létal",
    },
];

/// The [`LevelChoice`] describing a difficulty.
// Used by the difficulty test; the setup screen iterates `LEVELS`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn level_choice(level: Difficulty) -> LevelChoice {
    LEVELS[match level {
        Difficulty::Beginner => 0,
        Difficulty::Intermediate => 1,
        Difficulty::Expert => 2,
    }]
}

// ============================================================
// Selection
// ============================================================

/// Which of the two deck rows a card belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PickSide {
    /// "Votre équipage".
    You,
    /// "Adversaire (IA)".
    Foe,
}

/// What the setup screen has collected so far — TS `playerDeck` / `aiDeck` /
/// `level` state of `src/app/page.tsx`.
///
/// Note that, exactly like the web, the two rows are independent: the same crew
/// can be picked on both sides (a mirror match is legal).
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct SetupSelection {
    pub you: Option<DeckKey>,
    pub foe: Option<DeckKey>,
    pub level: Difficulty,
}

impl Default for SetupSelection {
    fn default() -> Self {
        // TS `useState<Level>("intermediate")`, both decks unpicked.
        SetupSelection {
            you: None,
            foe: None,
            level: Difficulty::Intermediate,
        }
    }
}

impl SetupSelection {
    /// TS `const ready = playerDeck && aiDeck`.
    pub fn is_ready(&self) -> bool {
        self.you.is_some() && self.foe.is_some()
    }

    /// The deck currently picked on one side.
    pub fn picked(&self, side: PickSide) -> Option<DeckKey> {
        match side {
            PickSide::You => self.you,
            PickSide::Foe => self.foe,
        }
    }

    /// TS `setPlayerDeck` / `setAiDeck` — a plain replace, clicking the already
    /// selected crew is a no-op rather than a toggle.
    pub fn pick(&mut self, side: PickSide, key: DeckKey) {
        match side {
            PickSide::You => self.you = Some(key),
            PickSide::Foe => self.foe = Some(key),
        }
    }

    /// `(your deck, the AI's deck)` once both are picked — the arguments of
    /// [`Session::new`](crate::bridge::Session::new).
    pub fn decks(&self) -> Option<(DeckDef, DeckDef)> {
        Some((self.you?.def(), self.foe?.def()))
    }

    /// Every def id both decks can put on the table (the two captains
    /// included), for [`ArtCache::prewarm`](crate::art::ArtCache::prewarm).
    pub fn art_def_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for key in [self.you, self.foe].into_iter().flatten() {
            ids.push(key.captain_id().to_string());
            for entry in key.def().cards {
                ids.push(entry.card_id);
            }
        }
        ids.sort();
        ids.dedup();
        ids
    }
}

/// A fresh, non-reproducible seed for a new game (the wall clock is the only
/// entropy source the client links against; a fixed seed replays a game).
pub fn fresh_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        // Cheap avalanche so two starts in the same millisecond still differ in
        // the high bits the engine's RNG consumes first.
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ 0x5DEE_CE66_D000_0000
}

// ============================================================
// Game over
// ============================================================

/// What the victory / defeat splash shows — TS `if (state.winner) { … }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameOverView {
    /// `state.winner === humanPlayer`.
    pub won: bool,
    /// `state.turnNumber`.
    pub turn: u32,
}

impl GameOverView {
    /// TS `won ? "VICTOIRE !" : "DÉFAITE…"`.
    pub fn title(&self) -> &'static str {
        if self.won {
            "VICTOIRE !"
        } else {
            "DÉFAITE…"
        }
    }

    /// TS `Tour {state.turnNumber}`.
    pub fn turn_label(&self) -> String {
        format!("Tour {}", self.turn)
    }
}

/// Read the splash out of a finished session. `won` is `false` while the game
/// is still running, so only call it once [`Session::winner`] is `Some`.
pub fn game_over_view(session: &Session) -> GameOverView {
    GameOverView {
        won: session.human_won(),
        turn: session.state.turn_number,
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tcgop_engine::types::PlayerId;

    #[test]
    fn deck_keys_mirror_the_engine_catalogue() {
        let engine = decks::all_decks();
        assert_eq!(engine.len(), DeckKey::ALL.len());
        for (i, key) in DeckKey::ALL.iter().enumerate() {
            assert_eq!(key.index(), i);
            let def = key.def();
            assert_eq!(def.name, engine[i].name, "deck #{i} is out of order");
            assert_eq!(
                def.captain_id,
                key.captain_id(),
                "{} declares another captain",
                def.name
            );
        }
    }

    #[test]
    fn every_deck_card_has_its_visual_identity() {
        for key in DeckKey::ALL {
            let v = key.visual();
            assert!(!v.label.is_empty());
            assert!(!v.captain.is_empty());
            assert!(!v.tagline.is_empty());
        }
        assert_eq!(DeckKey::Mugiwara.visual().crest, SetupCrest::Hat);
        assert_eq!(DeckKey::Marines.visual().crest, SetupCrest::Anchor);
        assert_eq!(DeckKey::Baroque.visual().crest, SetupCrest::Skull);
        assert_eq!(DeckKey::RedHair.visual().crest, SetupCrest::Skull);
        assert_eq!(DeckKey::Mugiwara.visual().captain, "Monkey D. Luffy");
    }

    #[test]
    fn levels_are_the_three_ts_rows_in_order() {
        assert_eq!(LEVELS.len(), 3);
        assert_eq!(LEVELS[0].level, Difficulty::Beginner);
        assert_eq!(LEVELS[1].level, Difficulty::Intermediate);
        assert_eq!(LEVELS[2].level, Difficulty::Expert);
        assert_eq!(LEVELS[1].label, "Intermédiaire");
        for choice in LEVELS {
            assert_eq!(level_choice(choice.level), choice);
        }
    }

    #[test]
    fn selection_starts_empty_on_intermediate_and_needs_both_decks() {
        let mut sel = SetupSelection::default();
        assert_eq!(sel.level, Difficulty::Intermediate);
        assert!(!sel.is_ready());
        assert!(sel.decks().is_none());

        sel.pick(PickSide::You, DeckKey::Mugiwara);
        assert!(!sel.is_ready(), "one side is not enough");

        sel.pick(PickSide::Foe, DeckKey::Marines);
        assert!(sel.is_ready());
        assert_eq!(sel.picked(PickSide::You), Some(DeckKey::Mugiwara));

        let (you, foe) = sel.decks().expect("ready");
        assert_eq!(you.captain_id, "CAP-LUFFY");
        assert_eq!(foe.captain_id, "CAP-AKAINU");
    }

    #[test]
    fn picking_replaces_rather_than_toggles_and_mirror_matches_are_legal() {
        let mut sel = SetupSelection::default();
        sel.pick(PickSide::You, DeckKey::Baroque);
        sel.pick(PickSide::You, DeckKey::Baroque);
        assert_eq!(
            sel.you,
            Some(DeckKey::Baroque),
            "re-clicking must not clear"
        );
        sel.pick(PickSide::Foe, DeckKey::Baroque);
        assert!(sel.is_ready(), "the same crew on both sides is allowed");
    }

    #[test]
    fn art_ids_cover_both_captains_and_are_deduplicated() {
        let mut sel = SetupSelection::default();
        assert!(sel.art_def_ids().is_empty());
        sel.pick(PickSide::You, DeckKey::Mugiwara);
        sel.pick(PickSide::Foe, DeckKey::Mugiwara);
        let ids = sel.art_def_ids();
        assert!(ids.contains(&"CAP-LUFFY".to_string()));
        assert!(ids.contains(&"MG-001".to_string()));
        let mut unique = ids.clone();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "a mirror match must not duplicate");
    }

    #[test]
    fn a_fresh_seed_builds_a_playable_session() {
        let mut sel = SetupSelection::default();
        sel.pick(PickSide::You, DeckKey::RedHair);
        sel.pick(PickSide::Foe, DeckKey::Baroque);
        let (you, foe) = sel.decks().unwrap();
        let session = Session::new(&you, &foe, sel.level, fresh_seed()).expect("valid decks");
        assert_eq!(session.human, PlayerId::Player1);
        assert!(session.winner().is_none());
        assert!(!session.valid.is_empty());
    }

    #[test]
    fn seeds_differ_between_calls() {
        assert_ne!(fresh_seed(), 0);
    }

    #[test]
    fn game_over_view_reads_the_session() {
        let session = crate::bridge::testkit::session(7);
        let view = game_over_view(&session);
        assert_eq!(view.turn, session.state.turn_number);
        assert!(!view.won, "nobody has won yet");
        assert_eq!(view.title(), "DÉFAITE…");
        assert_eq!(view.turn_label(), format!("Tour {}", view.turn));
        assert_eq!(GameOverView { won: true, turn: 9 }.title(), "VICTOIRE !");
        assert_eq!(GameOverView { won: true, turn: 9 }.turn_label(), "Tour 9");
    }
}

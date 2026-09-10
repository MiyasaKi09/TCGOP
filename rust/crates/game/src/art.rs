//! Art mapping — def id → illustration, faction visuals, and a lazy image
//! handle cache.
//!
//! Ported 1:1 from `src/data/cardArt.ts` (`CARD_ART`, `CARD_ART_VERSO`,
//! `CARD_ART_FOCUS`) and the faction half of `src/lib/theme.ts`. Paths are the
//! TS paths minus the leading `/`, because Bevy resolves them against
//! `crates/game/assets/`.
//!
//! Rendering rule for a full-illustration tile: draw the image cropped to the
//! tile with `object-fit: cover` semantics, anchored on [`Focus`] — the same
//! `background-position` the web client uses, so faces stay in frame.

use std::collections::HashMap;
use bevy::prelude::*;
use tcgop_engine::types::{Faction, Rarity, Trait};

// ============================================================
// Plugin
// ============================================================

/// Inserts the [`ArtCache`].
pub struct ArtPlugin;

impl Plugin for ArtPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ArtCache>();
    }
}

// ============================================================
// Static assets
// ============================================================

/// The sea texture used behind everything.
pub const SEA_IMAGE: &str = "decks/sea.png";
/// Top-down ship deck art for the pirate half.
pub const MUGIWARA_DECK_IMAGE: &str = "decks/mugiwara-deck-v.png";
/// Top-down ship deck art for the marine half.
pub const MARINE_DECK_IMAGE: &str = "decks/marine-deck-v.png";

// ============================================================
// Focus point
// ============================================================

/// Normalised focal point of an illustration (`0,0` = top-left, `1,1` =
/// bottom-right) — TS `CARD_ART_FOCUS` (`"50% 22%"` → `Focus { x: 0.5, y: 0.22 }`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Focus {
    pub x: f32,
    pub y: f32,
}

impl Focus {
    pub const CENTER: Focus = Focus { x: 0.5, y: 0.5 };

    pub const fn new(x: f32, y: f32) -> Self {
        Focus { x, y }
    }
}

impl Default for Focus {
    fn default() -> Self {
        Focus::CENTER
    }
}

// ============================================================
// def id → art
// ============================================================

/// TS `CARD_ART[defId]` — the recto (default) full illustration.
pub fn card_art(def_id: &str) -> Option<&'static str> {
    Some(match def_id {
        // --- Mugiwara characters ---
        "MG-001" => "cards/zoro.jpg",
        "MG-002" => "cards/sanji.jpg",
        "MG-003" => "cards/nami.jpg",
        "MG-004" => "cards/usopp.jpg",
        "MG-005" => "cards/chopper.jpg",
        "MG-006" => "cards/robin.jpg",
        "MG-007" => "cards/franky.jpg",
        "MG-008" => "cards/brook.jpg",
        // --- Weapons ---
        "MG-009" => "cards/wado.jpg",
        "MG-010" => "cards/sandai.jpg",
        "MG-011" => "cards/yubashiri.jpg",
        "MG-012" => "cards/clima-tact.jpg",
        "MG-013" => "cards/kabuto.jpg",
        // --- Devil Fruits (recto face) ---
        "MG-014" => "cards/gomu-recto.jpg",
        "MG-015" => "cards/hana-recto.jpg",
        "MG-016" => "cards/yomi-recto.jpg",
        // --- Accessories ---
        "MG-017" => "cards/baril.jpg",
        "MG-018" => "cards/dial.jpg",
        "MG-019" => "cards/vivre.jpg",
        // --- Ships ---
        "MG-020" => "cards/going-merry.jpg",
        "MG-021" => "cards/thousand-sunny.jpg",
        // --- Events ---
        "MG-022" => "cards/volonte-d.jpg",
        "MG-023" => "cards/nakama.jpg",
        "MG-024" => "cards/promesse.jpg",
        "MG-025" => "cards/tempete.jpg",
        "MG-028" => "cards/coup-de-burst.jpg",
        // --- Counters ---
        "MG-026" => "cards/je-veux-vivre.jpg",
        "MG-027" => "cards/drapeau-noir.jpg",
        // --- Captain Luffy (recto) ---
        "CAP-LUFFY" => "cards/luffy-recto.jpg",
        // --- Marines (ST02) ---
        "MR-001" => "cards/coby.jpg",
        "MR-002" => "cards/helmeppo.jpg",
        "MR-003" => "cards/tashigi.jpg",
        "MR-004" => "cards/smoker.jpg",
        "MR-005" => "cards/sentomaru.jpg",
        "MR-006" => "cards/momonga.jpg",
        "MR-007" => "cards/garp.jpg",
        "MR-008" => "cards/kizaru.jpg",
        "MR-009" => "cards/aokiji.jpg",
        "MR-010" => "cards/sengoku.jpg",
        // --- Captain Akainu (recto) ---
        "CAP-AKAINU" => "cards/akainu-recto.jpg",
        _ => return None,
    })
}

/// TS `CARD_ART_VERSO[defId]` — used when a fruit is awakened or a captain is
/// flipped.
pub fn card_art_verso(def_id: &str) -> Option<&'static str> {
    Some(match def_id {
        "MG-014" => "cards/gomu-verso.jpg",
        "MG-015" => "cards/hana-verso.jpg",
        "MG-016" => "cards/yomi-verso.jpg",
        "CAP-LUFFY" => "cards/luffy-verso.jpg",
        "CAP-AKAINU" => "cards/akainu-verso.jpg",
        _ => return None,
    })
}

/// TS `flipped ? (CARD_ART_VERSO[id] ?? CARD_ART[id]) : CARD_ART[id]`.
pub fn art_for(def_id: &str, flipped: bool) -> Option<&'static str> {
    if flipped {
        card_art_verso(def_id).or_else(|| card_art(def_id))
    } else {
        card_art(def_id)
    }
}

/// TS `CARD_ART_FOCUS[defId]`, defaulting to dead centre.
pub fn card_art_focus(def_id: &str) -> Focus {
    match def_id {
        "MG-001" => Focus::new(0.50, 0.22),
        "MG-002" => Focus::new(0.50, 0.16),
        "MG-003" => Focus::new(0.50, 0.30),
        "MG-004" => Focus::new(0.60, 0.42),
        "MG-005" => Focus::new(0.50, 0.30),
        "MG-006" => Focus::new(0.55, 0.30),
        "MG-007" => Focus::new(0.50, 0.26),
        "MG-008" => Focus::new(0.50, 0.16),
        "CAP-LUFFY" => Focus::new(0.50, 0.24),
        "CAP-AKAINU" => Focus::new(0.50, 0.24),
        "MR-001" => Focus::new(0.50, 0.22),
        "MR-002" => Focus::new(0.50, 0.22),
        "MR-003" => Focus::new(0.50, 0.22),
        "MR-004" => Focus::new(0.50, 0.22),
        "MR-005" => Focus::new(0.50, 0.26),
        "MR-006" => Focus::new(0.50, 0.28),
        "MR-007" => Focus::new(0.50, 0.24),
        "MR-008" => Focus::new(0.50, 0.26),
        "MR-009" => Focus::new(0.50, 0.24),
        "MR-010" => Focus::new(0.50, 0.26),
        _ => Focus::CENTER,
    }
}

// ============================================================
// Labels
// ============================================================

/// TS `TRAIT_LABEL` — the French keyword shown on badges and in card details.
pub fn trait_label(t: Trait) -> &'static str {
    match t {
        Trait::Shield => "Bouclier",
        Trait::Range => "Portée",
        Trait::Stealth => "Furtif",
        Trait::Rush => "Rush",
        Trait::Cursed => "Maudit",
        Trait::Logia => "Logia",
        Trait::Piercing => "Perçant",
        Trait::Conqueror => "Conquérant",
    }
}

/// TS `QUOTE[defId]` — flavour line shown next to the name on a detailed card.
pub fn quote(def_id: &str) -> Option<&'static str> {
    Some(match def_id {
        "MG-001" => "Rien ne s'est passé.",
        "MG-002" => "Jamais contre une femme.",
        "MG-003" => "Laisse-moi gérer la météo.",
        "MG-004" => "Capitaine aux 8000 hommes !",
        "MG-005" => "Un monstre pour toi !",
        "MG-006" => "Je veux vivre !",
        "MG-007" => "SUPER !",
        "MG-008" => "Yohohoho !",
        _ => return None,
    })
}

// ============================================================
// Faction visuals
// ============================================================

/// The crest glyph of a faction (drawn by the board / header, see `icons.tsx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crest {
    /// Straw hat — pirates.
    Hat,
    /// Anchor — marines.
    Anchor,
}

/// TS `FactionVisual` — everything a half of the terrain needs to look like its
/// owner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FactionVisual {
    /// Bright accent (labels, edges).
    pub accent: Color,
    /// Border colour for tokens with no rarity override.
    pub border: Color,
    /// Soft ambiance wash over that player's half.
    pub ambiance: Color,
    /// Crest glyph.
    pub crest: Crest,
    /// Top-down ship deck art (bow up) used as the half's floor.
    pub ship_deck: &'static str,
    /// Display label.
    pub label: &'static str,
}

const PIRATE_VISUAL: FactionVisual = FactionVisual {
    accent: Color::srgb(0.910, 0.584, 0.290), // #e8954a
    border: Color::srgba(0.910, 0.584, 0.290, 0.60),
    ambiance: Color::srgba(0.910, 0.584, 0.290, 0.18),
    crest: Crest::Hat,
    ship_deck: MUGIWARA_DECK_IMAGE,
    label: "Mugiwara",
};

const MARINE_VISUAL: FactionVisual = FactionVisual {
    accent: Color::srgb(0.357, 0.592, 0.847), // #5b97d8
    border: Color::srgba(0.243, 0.471, 0.722, 0.60),
    ambiance: Color::srgba(0.243, 0.471, 0.722, 0.18),
    crest: Crest::Anchor,
    ship_deck: MARINE_DECK_IMAGE,
    label: "Marine",
};

/// TS `faction(key)` — revolutionary reads as pirate, independent as marine.
pub fn faction_visual(faction: Faction) -> FactionVisual {
    match faction {
        Faction::Pirate | Faction::Revolutionary => PIRATE_VISUAL,
        Faction::Marine | Faction::Independent => MARINE_VISUAL,
    }
}

/// TS `RARITY_BORDER[rarity]` — token / card border colour.
pub fn rarity_border(rarity: Rarity) -> Color {
    match rarity {
        Rarity::C => Color::srgba(0.647, 0.667, 0.706, 0.55),
        Rarity::U => Color::srgba(0.290, 0.659, 0.420, 0.60),
        Rarity::R => Color::srgba(0.231, 0.510, 0.769, 0.60),
        Rarity::Sr => Color::srgba(0.659, 0.373, 0.816, 0.65),
        Rarity::L => Color::srgba(0.910, 0.722, 0.294, 0.80),
        Rarity::Cap => Color::srgba(0.910, 0.722, 0.294, 0.85),
    }
}

/// TS `TRAIT_COLOR[trait]` — dot / pill colour on board tokens and details.
pub fn trait_color(t: Trait) -> Color {
    match t {
        Trait::Shield => Color::srgb(0.761, 0.573, 0.353),  // #c2925a
        Trait::Range => Color::srgb(0.878, 0.533, 0.235),   // #e0883c
        Trait::Stealth => Color::srgb(0.616, 0.647, 0.710), // #9da5b5
        Trait::Rush => Color::srgb(0.937, 0.376, 0.365),    // #ef605d
        Trait::Cursed => Color::srgb(0.659, 0.373, 0.816),  // #a85fd0
        Trait::Logia => Color::srgb(0.498, 0.812, 0.918),   // #7fcfea
        Trait::Piercing => Color::srgb(0.910, 0.722, 0.294),
        Trait::Conqueror => Color::srgb(0.910, 0.722, 0.294),
    }
}

// ============================================================
// Handle cache
// ============================================================

/// Lazy `Handle<Image>` cache keyed by asset path.
///
/// `AssetServer::load` already de-duplicates, but going through the cache keeps
/// the strong handles alive for the whole session (so a tile that is despawned
/// and respawned never re-decodes its JPEG) and gives one obvious place to
/// pre-warm the art of a deck.
#[derive(Resource, Default)]
pub struct ArtCache {
    images: HashMap<String, Handle<Image>>,
}

impl ArtCache {
    /// Handle for an asset path, loading it on first use.
    pub fn image(&mut self, assets: &AssetServer, path: &str) -> Handle<Image> {
        if let Some(handle) = self.images.get(path) {
            return handle.clone();
        }
        let handle: Handle<Image> = assets.load(path.to_string());
        self.images.insert(path.to_string(), handle.clone());
        handle
    }

    /// Handle for a card's illustration (`None` when the def has no art).
    pub fn card(&mut self, assets: &AssetServer, def_id: &str, flipped: bool) -> Option<Handle<Image>> {
        art_for(def_id, flipped).map(|path| self.image(assets, path))
    }

    /// Handle for a faction's ship deck floor.
    pub fn ship_deck(&mut self, assets: &AssetServer, faction: Faction) -> Handle<Image> {
        self.image(assets, faction_visual(faction).ship_deck)
    }

    /// Load everything referenced by a list of def ids up front.
    pub fn prewarm<'a>(&mut self, assets: &AssetServer, def_ids: impl IntoIterator<Item = &'a str>) {
        self.image(assets, SEA_IMAGE);
        self.image(assets, MUGIWARA_DECK_IMAGE);
        self.image(assets, MARINE_DECK_IMAGE);
        for id in def_ids {
            if let Some(path) = card_art(id) {
                self.image(assets, path);
            }
            if let Some(path) = card_art_verso(id) {
                self.image(assets, path);
            }
        }
    }

    /// Number of cached handles (tests / diagnostics).
    pub fn len(&self) -> usize {
        self.images.len()
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_card_art_path_exists_on_disk() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/");
        let ids = [
            "MG-001", "MG-014", "MG-020", "MG-026", "CAP-LUFFY", "MR-010", "CAP-AKAINU",
        ];
        for id in ids {
            let path = card_art(id).unwrap_or_else(|| panic!("{id} has no art"));
            assert!(
                std::path::Path::new(&format!("{root}{path}")).exists(),
                "missing asset {path}"
            );
        }
        for path in [SEA_IMAGE, MUGIWARA_DECK_IMAGE, MARINE_DECK_IMAGE] {
            assert!(std::path::Path::new(&format!("{root}{path}")).exists());
        }
    }

    #[test]
    fn verso_art_falls_back_to_recto() {
        assert_eq!(art_for("CAP-LUFFY", true), Some("cards/luffy-verso.jpg"));
        assert_eq!(art_for("MG-001", true), Some("cards/zoro.jpg"));
        assert_eq!(art_for("nope", false), None);
    }

    #[test]
    fn focus_defaults_to_centre() {
        assert_eq!(card_art_focus("MG-004"), Focus::new(0.60, 0.42));
        assert_eq!(card_art_focus("MG-020"), Focus::CENTER);
    }

    #[test]
    fn faction_visuals_pick_the_right_deck() {
        assert_eq!(
            faction_visual(Faction::Pirate).ship_deck,
            MUGIWARA_DECK_IMAGE
        );
        assert_eq!(
            faction_visual(Faction::Revolutionary).crest,
            Crest::Hat
        );
        assert_eq!(faction_visual(Faction::Marine).ship_deck, MARINE_DECK_IMAGE);
        assert_eq!(faction_visual(Faction::Independent).crest, Crest::Anchor);
    }
}

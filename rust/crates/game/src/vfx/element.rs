//! Combat look-up table — a 1:1 port of `src/lib/vfx.ts`.
//!
//! Pure data: one coherent colour / glow / tint / glyph per element, plus the
//! two "archetype" looks (`physical` and `haki`) the TS table carries.
//!
//! **Glyphs.** The web table uses emoji (🔥 💧 🌪). None of the shipped fonts
//! covers them — Cinzel / Oswald / Spectral / Bangers stop at Latin-1 and even
//! `DejaVuSans.ttf` (the symbol fallback, see [`SYMBOL_FONT_PATH`](crate::vfx::SYMBOL_FONT_PATH))
//! has no `U+1F525`. Fire, water and sand therefore use the closest glyph that
//! DejaVu actually contains (`✹`, `≈`, `✳`); thunder, ice, poison, physical and
//! haki keep the exact web glyph.

use bevy::prelude::*;
use tcgop_engine::types::Element;

/// TS `VfxElement = Element | "physical" | "haki"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum VfxElement {
    Fire,
    Water,
    Thunder,
    Ice,
    Sand,
    Poison,
    /// No element and no Haki — the plain gold hit.
    #[default]
    Physical,
    /// No element but Haki is up — the purple hit.
    Haki,
}

/// TS `ElementStyle`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElementStyle {
    pub key: VfxElement,
    /// French label of the element ("" for `physical`, like the TS table).
    pub label: &'static str,
    /// Primary colour (projectile core, damage number, burst ring).
    pub color: Color,
    /// Soft glow around the projectile / burst.
    pub glow: Color,
    /// Full-screen tint flash on a big hit.
    pub tint: Color,
    /// Glyph carried by the projectile and the burst.
    pub glyph: &'static str,
}

const fn hex(r: u8, g: u8, b: u8) -> Color {
    Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

const fn hexa(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::srgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

/// The style of one look — TS `ELEMENT_STYLE[key]`.
pub const fn style(key: VfxElement) -> ElementStyle {
    match key {
        VfxElement::Fire => ElementStyle {
            key,
            label: "Feu",
            color: hex(0xE0, 0x65, 0x3C),
            glow: hexa(0xE0, 0x65, 0x3C, 0.75),
            tint: hexa(0xE0, 0x65, 0x3C, 0.16),
            glyph: "\u{2739}",
        },
        VfxElement::Water => ElementStyle {
            key,
            label: "Eau",
            color: hex(0x3C, 0x9F, 0xE0),
            glow: hexa(0x3C, 0x9F, 0xE0, 0.70),
            tint: hexa(0x3C, 0x9F, 0xE0, 0.16),
            glyph: "\u{2248}",
        },
        VfxElement::Thunder => ElementStyle {
            key,
            label: "Foudre",
            color: hex(0xF2, 0xD0, 0x4B),
            glow: hexa(0xF2, 0xD0, 0x4B, 0.80),
            tint: hexa(0xF2, 0xD0, 0x4B, 0.18),
            glyph: "\u{26A1}",
        },
        VfxElement::Ice => ElementStyle {
            key,
            label: "Glace",
            color: hex(0x7F, 0xE0, 0xF0),
            glow: hexa(0x7F, 0xE0, 0xF0, 0.70),
            tint: hexa(0x7F, 0xE0, 0xF0, 0.16),
            glyph: "\u{2744}",
        },
        VfxElement::Sand => ElementStyle {
            key,
            label: "Sable",
            color: hex(0xD8, 0xA8, 0x5A),
            glow: hexa(0xD8, 0xA8, 0x5A, 0.70),
            tint: hexa(0xD8, 0xA8, 0x5A, 0.16),
            glyph: "\u{2733}",
        },
        VfxElement::Poison => ElementStyle {
            key,
            label: "Poison",
            color: hex(0x9F, 0xCB, 0x4A),
            glow: hexa(0x9F, 0xCB, 0x4A, 0.70),
            tint: hexa(0x9F, 0xCB, 0x4A, 0.15),
            glyph: "\u{2620}",
        },
        VfxElement::Physical => ElementStyle {
            key,
            label: "",
            color: hex(0xF0, 0xD2, 0x7A),
            glow: hexa(0xF0, 0xD2, 0x7A, 0.60),
            tint: hexa(0xF0, 0xD2, 0x7A, 0.08),
            glyph: "\u{2726}",
        },
        VfxElement::Haki => ElementStyle {
            key,
            label: "Haki",
            color: hex(0xB5, 0x7B, 0xE6),
            glow: hexa(0xB5, 0x7B, 0xE6, 0.75),
            tint: hexa(0x1E, 0x08, 0x30, 0.22),
            glyph: "\u{2726}",
        },
    }
}

/// TS `HEAL_COLOR`.
pub const HEAL_COLOR: Color = hex(0x5B, 0xC4, 0x6A);

/// Glyph of a healing burst (`✚`, as in `CombatVfxLayer`).
pub const HEAL_GLYPH: &str = "\u{271A}";

/// Glyph of a KO shatter (`✖`, as in `CombatVfxLayer`).
pub const KO_GLYPH: &str = "\u{2716}";

impl From<Element> for VfxElement {
    fn from(value: Element) -> Self {
        match value {
            Element::Fire => VfxElement::Fire,
            Element::Water => VfxElement::Water,
            Element::Thunder => VfxElement::Thunder,
            Element::Ice => VfxElement::Ice,
            Element::Sand => VfxElement::Sand,
            Element::Poison => VfxElement::Poison,
        }
    }
}

impl VfxElement {
    /// This look's style row.
    pub const fn style(self) -> ElementStyle {
        style(self)
    }
}

/// TS `styleFor(element, hasHaki)` — explicit element wins, else Haki, else
/// physical.
// TS `styleFor`, kept under its own name for the port test; the vfx
// pipeline calls `element_for(..).style()`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn style_for(element: Option<Element>, has_haki: bool) -> ElementStyle {
    element_for(element, has_haki).style()
}

/// The key `styleFor` would resolve to.
pub fn element_for(element: Option<Element>, has_haki: bool) -> VfxElement {
    match element {
        Some(e) => VfxElement::from(e),
        None if has_haki => VfxElement::Haki,
        None => VfxElement::Physical,
    }
}

/// TS `eventElement(defId)` — the AoE flair of an event card.
// TS `eventElement`: defined in `src/lib/vfx.ts` but never called there
// either — the port keeps the table (and its test) rather than drop it.
#[cfg_attr(not(test), allow(dead_code))]
pub fn event_element(def_id: &str) -> VfxElement {
    match def_id {
        // Tempête de Sable
        "BW-023" => VfxElement::Sand,
        // Buster Call
        "MR-020" => VfxElement::Fire,
        // Tempête
        "MG-025" => VfxElement::Physical,
        _ => VfxElement::Physical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_element_wins_over_haki() {
        assert_eq!(
            style_for(Some(Element::Ice), true).key,
            VfxElement::Ice,
            "ice + haki must still look like ice"
        );
        assert_eq!(style_for(None, true).key, VfxElement::Haki);
        assert_eq!(style_for(None, false).key, VfxElement::Physical);
    }

    #[test]
    fn every_look_has_a_glyph_and_a_visible_colour() {
        for key in [
            VfxElement::Fire,
            VfxElement::Water,
            VfxElement::Thunder,
            VfxElement::Ice,
            VfxElement::Sand,
            VfxElement::Poison,
            VfxElement::Physical,
            VfxElement::Haki,
        ] {
            let st = style(key);
            assert_eq!(st.key, key);
            assert!(!st.glyph.is_empty());
            assert!(st.color.alpha() > 0.9, "the core colour is opaque");
            assert!(st.tint.alpha() < 0.3, "the screen tint stays subtle");
        }
    }

    #[test]
    fn event_cards_borrow_an_element() {
        assert_eq!(event_element("BW-023"), VfxElement::Sand);
        assert_eq!(event_element("MR-020"), VfxElement::Fire);
        assert_eq!(event_element("unknown"), VfxElement::Physical);
    }
}

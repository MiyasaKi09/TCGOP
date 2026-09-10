//! Colour tokens and tiny bundle helpers shared by every board widget.

use bevy::prelude::*;
use bevy::text::LetterSpacing;
use tcgop_engine::types::StatusEffectType;

use crate::app::Palette;
use crate::selection::StatusTone;

// ============================================================
// Colours
// ============================================================

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

/// TS `STATUS_COLOR` (`src/lib/theme.ts`) — the badge colour of a status.
pub fn status_color(effect: StatusEffectType) -> Color {
    match effect {
        StatusEffectType::Burn => rgb(0xE0, 0x65, 0x3C),
        StatusEffectType::Poison => rgb(0x8F, 0xB8, 0x4A),
        StatusEffectType::Freeze => rgb(0x5C, 0xC6, 0xE0),
        StatusEffectType::Desiccation => rgb(0xC2, 0x92, 0x5A),
        StatusEffectType::Trap => rgb(0xE0, 0x46, 0x3F),
        StatusEffectType::Immobilize => rgb(0xE8, 0x79, 0xB0),
        StatusEffectType::Sleep => rgb(0x9B, 0x8C, 0xE0),
        StatusEffectType::LoseAction => rgb(0xC9, 0xA8, 0x2E),
        StatusEffectType::SelfKo => rgb(0xE0, 0x46, 0x3F),
        StatusEffectType::NoStealth => rgb(0x7F, 0xB0, 0xE8),
        StatusEffectType::NoHeal => rgb(0xFF, 0x8A, 0x80),
    }
}

/// Colour of the header status tag, per [`StatusTone`].
pub fn tone_color(palette: &Palette, tone: StatusTone) -> Color {
    match tone {
        StatusTone::Ready => palette.hp,
        StatusTone::Waiting => palette.gold,
        StatusTone::Danger => palette.foe,
        StatusTone::Target => palette.target,
        StatusTone::Support => palette.def,
        StatusTone::Deploy => palette.deploy,
        StatusTone::Captain => palette.amber,
    }
}

/// Same colour, faded — used for the tag's pill background.
pub fn washed(color: Color, alpha: f32) -> Color {
    color.with_alpha(alpha)
}

// ============================================================
// Bundles
// ============================================================

/// A single-line, non-pickable text node.
pub fn text(value: impl Into<String>, font: &Handle<Font>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font: font.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        TextLayout::no_wrap(),
        Pickable::IGNORE,
    )
}

/// An upper-case, letter-spaced Oswald caption (the "LIGNE AVANT" style).
pub fn caption(
    value: impl Into<String>,
    font: &Handle<Font>,
    size: f32,
    color: Color,
    spacing: f32,
) -> impl Bundle {
    (
        text(value, font, size, color),
        LetterSpacing::Px(spacing),
    )
}

/// An absolutely positioned, non-pickable overlay filling its parent.
pub fn overlay(color: Color) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: px(0.0),
            right: px(0.0),
            top: px(0.0),
            bottom: px(0.0),
            ..default()
        },
        BackgroundColor(color),
        Pickable::IGNORE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tone_has_a_colour_from_the_palette() {
        let p = Palette::default();
        assert_eq!(tone_color(&p, StatusTone::Ready), p.hp);
        assert_eq!(tone_color(&p, StatusTone::Danger), p.foe);
        assert_eq!(tone_color(&p, StatusTone::Deploy), p.deploy);
        assert_ne!(
            tone_color(&p, StatusTone::Target),
            tone_color(&p, StatusTone::Support)
        );
    }

    #[test]
    fn status_colours_are_distinct_per_family() {
        assert_ne!(
            status_color(StatusEffectType::Burn),
            status_color(StatusEffectType::Freeze)
        );
        assert_eq!(
            status_color(StatusEffectType::Trap),
            status_color(StatusEffectType::SelfKo),
            "both are the alarm red of the TS table"
        );
    }
}

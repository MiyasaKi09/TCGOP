//! Small node builders shared by the setup and game-over screens.
//!
//! No icon font is involved: the crests are drawn with plain rounded nodes, the
//! same trick `board::widgets::Painter::crest` uses on the header.

use bevy::prelude::*;
use bevy::text::LetterSpacing;
use bevy::ui::widget::Text;

use crate::app::Fonts;

use super::model::SetupCrest;

/// Cinzel bold — screen titles and crew names.
pub fn title_font(fonts: &Fonts, size: f32) -> TextFont {
    TextFont {
        font: fonts.cinzel_bold.clone().into(),
        font_size: FontSize::Px(size),
        ..default()
    }
}

/// Oswald — every UI label.
pub fn label_font(fonts: &Fonts, size: f32) -> TextFont {
    TextFont {
        font: fonts.oswald.clone().into(),
        font_size: FontSize::Px(size),
        ..default()
    }
}

/// Oswald bold — buttons and emphasised labels.
pub fn bold_label_font(fonts: &Fonts, size: f32) -> TextFont {
    TextFont {
        font: fonts.oswald_bold.clone().into(),
        font_size: FontSize::Px(size),
        ..default()
    }
}

/// Spectral — flavour lines (the deck taglines and level subtitles).
pub fn flavour_font(fonts: &Fonts, size: f32) -> TextFont {
    TextFont {
        font: fonts.spectral.clone().into(),
        font_size: FontSize::Px(size),
        ..default()
    }
}

/// An all-caps section caption: `VOTRE ÉQUIPAGE`, `NIVEAU DE L'IA`, …
pub fn caption(fonts: &Fonts, text: &str, color: Color) -> impl Bundle {
    (
        Text::new(text.to_uppercase()),
        label_font(fonts, 11.0),
        LetterSpacing::Px(2.0),
        TextColor(color),
        Pickable::IGNORE,
    )
}

/// The crest of a crew, drawn as a ringed disc with an inner mark.
///
/// * [`SetupCrest::Hat`] — horizontal brim,
/// * [`SetupCrest::Anchor`] — vertical shank,
/// * [`SetupCrest::Skull`] — two eye sockets.
pub fn spawn_crest(parent: &mut ChildSpawnerCommands, crest: SetupCrest, size: f32, accent: Color) {
    let ring = (size / 7.0).max(1.5);
    parent
        .spawn((
            Node {
                width: px(size),
                height: px(size),
                flex_shrink: 0.0,
                border: UiRect::all(px(ring)),
                border_radius: BorderRadius::MAX,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                column_gap: px((size * 0.14).max(1.0)),
                ..default()
            },
            BorderColor::all(accent),
            BackgroundColor(accent.with_alpha(0.16)),
            Pickable::IGNORE,
        ))
        .with_children(|disc| {
            let bar = |w: f32, h: f32| {
                (
                    Node {
                        width: px(w),
                        height: px(h),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                    BackgroundColor(accent),
                    Pickable::IGNORE,
                )
            };
            match crest {
                SetupCrest::Hat => {
                    disc.spawn(bar(size * 0.58, (size * 0.14).max(1.5)));
                }
                SetupCrest::Anchor => {
                    disc.spawn(bar((size * 0.14).max(1.5), size * 0.58));
                }
                SetupCrest::Skull => {
                    let eye = (size * 0.2).max(2.0);
                    disc.spawn(bar(eye, eye));
                    disc.spawn(bar(eye, eye));
                }
            }
        });
}

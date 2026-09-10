//! The victory / defeat splash — port of the `if (state.winner)` early return
//! of `src/components/Game.tsx`.
//!
//! ```text
//!        ≡≡≡≡≡≡≡≡≡ speed lines ≡≡≡≡≡≡≡≡≡
//!               V I C T O I R E !
//!                    TOUR 12
//!                 (  Rejouer  )
//! ```
//!
//! *Rejouer* drops the [`Session`], returns the UI state machine to idle and
//! goes back to [`AppScreen::Setup`] — the native equivalent of the web's
//! `window.location.reload()`.

use bevy::prelude::*;
use bevy::text::LetterSpacing;
use bevy::ui::widget::Text;

use crate::app::{AppScreen, Fonts, Palette, layout as L};
use crate::art::{ArtCache, SEA_IMAGE};
use crate::bridge::Session;
use crate::selection::{SelectedHandCard, UiMode};

use super::model::{GameOverView, game_over_view};
use super::ReplayRequested;
use super::widgets::{bold_label_font, label_font, title_font};

/// "Rejouer".
#[derive(Component, Debug, Clone, Copy)]
pub struct ReplayButton;

const BAND_W: f32 = 660.0;
const BAND_H: f32 = 140.0;
/// Number of speed lines drawn behind the title.
const SPEED_LINES: usize = 11;

/// Build the splash. Runs on `OnEnter(AppScreen::GameOver)`.
// Every argument is a distinct Bevy system param (resources the splash reads
// and the two selection resources it must reset); merging them into a
// `SystemParam` bundle would only rename the borrow, not remove it.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_game_over(
    mut commands: Commands,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    assets: Option<Res<AssetServer>>,
    mut art: ResMut<ArtCache>,
    session: Option<Res<Session>>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
) {
    // The board is gone; no popover or half-built selection may survive it.
    *mode = UiMode::Idle;
    selected.0 = None;

    let view = session
        .as_deref()
        .map(game_over_view)
        .unwrap_or(GameOverView { won: false, turn: 0 });
    let pal = *palette;
    let accent = if view.won { pal.hp } else { pal.foe };

    commands
        .spawn((
            Name::new("GameOverScreen"),
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                width: percent(100.0),
                height: percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(26.0),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(pal.bg_deep),
            GlobalZIndex(L::Z_MODAL),
            DespawnOnExit(AppScreen::GameOver),
        ))
        .with_children(|screen| {
            // --- backdrop ---------------------------------------------------
            if let Some(assets) = assets.as_deref() {
                screen.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0.0),
                        top: px(0.0),
                        width: percent(100.0),
                        height: percent(100.0),
                        ..default()
                    },
                    ImageNode::new(art.image(assets, SEA_IMAGE))
                        .with_color(Color::WHITE.with_alpha(0.18))
                        .with_mode(NodeImageMode::Stretch),
                    Pickable::IGNORE,
                ));
            }
            screen.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    top: px(0.0),
                    width: percent(100.0),
                    height: percent(100.0),
                    ..default()
                },
                BackgroundGradient::from(LinearGradient::to_bottom(vec![
                    ColorStop::new(pal.bg_deep.with_alpha(0.70), percent(0.0)),
                    ColorStop::new(accent.with_alpha(0.14), percent(50.0)),
                    ColorStop::new(pal.bg_deep.with_alpha(0.92), percent(100.0)),
                ])),
                Pickable::IGNORE,
            ));

            // --- title over speed lines -------------------------------------
            screen
                .spawn((
                    Node {
                        width: px(BAND_W),
                        height: px(BAND_H),
                        max_width: percent(100.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|band| {
                    spawn_speed_lines(band, accent);
                    band.spawn((
                        Text::new(view.title()),
                        title_font(&fonts, 54.0),
                        LetterSpacing::Px(2.0),
                        TextColor(accent),
                        TextShadow {
                            offset: Vec2::new(0.0, 4.0),
                            color: Color::BLACK.with_alpha(0.8),
                        },
                        Pickable::IGNORE,
                    ));
                });

            // --- turn count --------------------------------------------------
            screen.spawn((
                Text::new(view.turn_label().to_uppercase()),
                label_font(&fonts, 17.0),
                LetterSpacing::Px(4.0),
                TextColor(pal.text_dim),
                Pickable::IGNORE,
            ));

            // --- replay ------------------------------------------------------
            screen
                .spawn((
                    ReplayButton,
                    Name::new("ReplayButton"),
                    Node {
                        width: px(268.0),
                        height: px(56.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border_radius: BorderRadius::all(px(L::BUTTON_RADIUS)),
                        ..default()
                    },
                    BackgroundGradient::from(LinearGradient::to_bottom_right(vec![
                        ColorStop::new(pal.gold, percent(0.0)),
                        ColorStop::new(pal.amber, percent(100.0)),
                    ])),
                ))
                .observe(replay_clicked)
                .with_children(|button| {
                    button.spawn((
                        Text::new("Rejouer"),
                        bold_label_font(&fonts, 19.0),
                        LetterSpacing::Px(1.0),
                        TextColor(pal.text_on_gold),
                        Pickable::IGNORE,
                    ));
                });
        });
}

/// The manga "speed lines" behind the title: thin horizontal streaks that fade
/// away from the centre line.
fn spawn_speed_lines(band: &mut ChildSpawnerCommands, accent: Color) {
    for i in 0..SPEED_LINES {
        // -1.0 (top) .. 1.0 (bottom)
        let t = (i as f32 / (SPEED_LINES - 1) as f32) * 2.0 - 1.0;
        let fade = 1.0 - t.abs();
        let width = BAND_W * (0.30 + 0.62 * (1.0 - fade));
        let alpha = 0.05 + 0.22 * fade;
        band.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px((BAND_W - width) * 0.5),
                top: px((BAND_H * 0.5) + t * (BAND_H * 0.42) - 1.0),
                width: px(width),
                height: px(if i % 2 == 0 { 2.0 } else { 1.0 }),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(accent.with_alpha(alpha)),
            Pickable::IGNORE,
        ));
    }
}

fn replay_clicked(click: On<Pointer<Click>>, mut requests: MessageWriter<ReplayRequested>) {
    if click.button != PointerButton::Primary {
        return;
    }
    requests.write(ReplayRequested);
}

/// TS `window.location.reload()` — start over from the deck picker.
pub(super) fn handle_replay_request(
    mut requests: MessageReader<ReplayRequested>,
    mut commands: Commands,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut next: ResMut<NextState<AppScreen>>,
) {
    if requests.read().count() == 0 {
        return;
    }
    *mode = UiMode::Idle;
    selected.0 = None;
    commands.remove_resource::<Session>();
    next.set(AppScreen::Setup);
}

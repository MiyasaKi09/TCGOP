//! The deck-selection screen — port of `src/app/page.tsx`.
//!
//! ```text
//!            ☠  One Piece Grand Line TCG
//!                  PRÉPARE LE DUEL
//!   VOTRE ÉQUIPAGE      [Mugiwara][Marine][Baroque][Red Hair]
//!   ADVERSAIRE (IA)     [Mugiwara][Marine][Baroque][Red Hair]
//!   NIVEAU DE L'IA      [Débutant][Intermédiaire][Expert]
//!                  ( Commencer le combat )
//! ```
//!
//! Clicking a crew stores it in [`SetupSelection`]; the button is only live
//! once both sides are picked (TS `disabled={!ready}`). Confirming builds the
//! [`Session`] with a fresh seed, pre-warms the art of both decks and enters
//! [`AppScreen::Board`].

use bevy::prelude::*;
use bevy::text::LetterSpacing;
use bevy::ui::widget::Text;

use crate::app::{AppScreen, Fonts, Palette, layout as L};
use crate::art::{ArtCache, SEA_IMAGE, card_art};
use crate::bridge::Session;
use crate::hand::SYMBOL_FONT_PATH;
use crate::selection::{SelectedHandCard, UiMode};

use super::model::{DeckKey, LEVELS, PickSide, SetupSelection, fresh_seed};
use super::StartGameRequested;
use super::widgets::{bold_label_font, caption, flavour_font, label_font, spawn_crest, title_font};

// ============================================================
// Markers
// ============================================================

/// One crew card of one of the two rows.
#[derive(Component, Debug, Clone, Copy)]
pub struct DeckCardButton {
    pub key: DeckKey,
    pub side: PickSide,
}

/// The "✓" pill in the corner of a selected crew card.
#[derive(Component, Debug, Clone, Copy)]
pub struct DeckCardCheck {
    pub key: DeckKey,
    pub side: PickSide,
}

/// One of the three difficulty buttons.
#[derive(Component, Debug, Clone, Copy)]
pub struct LevelButton(pub tcgop_engine::ai::Difficulty);

/// The label inside a difficulty button (it recolours with the selection).
#[derive(Component, Debug, Clone, Copy)]
pub struct LevelButtonLabel(pub tcgop_engine::ai::Difficulty);

/// "Commencer le combat".
#[derive(Component, Debug, Clone, Copy)]
pub struct StartButton;

/// Its caption (recoloured when the button goes live).
#[derive(Component, Debug, Clone, Copy)]
pub struct StartButtonLabel;

// ============================================================
// Layout constants (this screen only)
// ============================================================

const CARD_W: f32 = 224.0;
const CARD_GAP: f32 = 14.0;
const ACCENT_STRIP_H: f32 = 6.0;
const LEVEL_W: f32 = 186.0;
const START_W: f32 = 306.0;
const START_H: f32 = 52.0;

/// U+2713 CHECK MARK — the TS `✓` badge on the selected crew.
const CHECK: &str = "\u{2713}";

// ============================================================
// Spawn
// ============================================================

/// Build the whole screen. Runs on `OnEnter(AppScreen::Setup)`.
pub(super) fn spawn_setup(
    mut commands: Commands,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    assets: Option<Res<AssetServer>>,
    mut art: ResMut<ArtCache>,
    mut selection: ResMut<SetupSelection>,
) {
    // Leaving the board and coming back must not keep the previous picks
    // half-applied; the web remounts the page.
    *selection = SetupSelection::default();

    let pal = *palette;
    let fonts = fonts.clone();
    // The four display families stop at Latin-1, so the "✓" pill borrows the
    // hand's symbol face (head-lessly it stays `Handle::default()`).
    let check_font: Handle<Font> = assets
        .as_deref()
        .map(|assets| assets.load(SYMBOL_FONT_PATH))
        .unwrap_or_default();

    let mut root = commands.spawn((
        Name::new("SetupScreen"),
        Node {
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            width: percent(100.0),
            height: percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(16.0),
            padding: UiRect::axes(px(24.0), px(20.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(pal.bg_deep),
        GlobalZIndex(L::Z_MODAL),
        DespawnOnExit(AppScreen::Setup),
    ));

    root.with_children(|screen| {
        // --- sea backdrop -------------------------------------------------
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
                    .with_color(Color::WHITE.with_alpha(0.22))
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
                ColorStop::new(pal.bg_deep.with_alpha(0.62), percent(0.0)),
                ColorStop::new(pal.bg_deep.with_alpha(0.92), percent(100.0)),
            ])),
            Pickable::IGNORE,
        ));

        // --- title --------------------------------------------------------
        spawn_title(screen, &pal, &fonts);

        // --- the two crew rows --------------------------------------------
        spawn_deck_row(
            screen,
            &pal,
            &fonts,
            assets.as_deref(),
            &mut art,
            PickSide::You,
            "Votre équipage",
            pal.hp,
            &check_font,
        );
        spawn_deck_row(
            screen,
            &pal,
            &fonts,
            assets.as_deref(),
            &mut art,
            PickSide::Foe,
            "Adversaire (IA)",
            pal.foe,
            &check_font,
        );

        // --- difficulty ----------------------------------------------------
        spawn_level_row(screen, &pal, &fonts);

        // --- CTA -----------------------------------------------------------
        spawn_start_button(screen, &pal, &fonts);
    });
}

fn spawn_title(screen: &mut ChildSpawnerCommands, pal: &Palette, fonts: &Fonts) {
    screen
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(6.0),
                margin: UiRect::bottom(px(4.0)),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|block| {
            block
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: px(12.0),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|line| {
                    spawn_crest(line, super::model::SetupCrest::Skull, 26.0, pal.gold);
                    line.spawn((
                        Text::new("One Piece Grand Line TCG"),
                        title_font(fonts, 32.0),
                        LetterSpacing::Px(1.0),
                        TextColor(pal.gold),
                        TextShadow {
                            offset: Vec2::new(0.0, 3.0),
                            color: Color::BLACK.with_alpha(0.75),
                        },
                        Pickable::IGNORE,
                    ));
                });
            block.spawn(caption(fonts, "Prépare le duel", pal.text_dim));
        });
}

#[allow(clippy::too_many_arguments)]
fn spawn_deck_row(
    screen: &mut ChildSpawnerCommands,
    pal: &Palette,
    fonts: &Fonts,
    assets: Option<&AssetServer>,
    art: &mut ArtCache,
    side: PickSide,
    label: &str,
    label_color: Color,
    check_font: &Handle<Font>,
) {
    screen
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(8.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|section| {
            section.spawn(caption(fonts, label, label_color));
            section
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::Center,
                        column_gap: px(CARD_GAP),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|row| {
                    for key in DeckKey::ALL {
                        spawn_deck_card(row, pal, fonts, assets, art, side, key, check_font);
                    }
                });
        });
}

#[allow(clippy::too_many_arguments)]
fn spawn_deck_card(
    row: &mut ChildSpawnerCommands,
    pal: &Palette,
    fonts: &Fonts,
    assets: Option<&AssetServer>,
    art: &mut ArtCache,
    side: PickSide,
    key: DeckKey,
    check_font: &Handle<Font>,
) {
    let visual = key.visual();
    let art_handle = assets
        .zip(card_art(key.captain_id()))
        .map(|(assets, path)| art.image(assets, path));

    row.spawn((
        DeckCardButton { key, side },
        Name::new(format!("DeckCard({:?}/{:?})", side, key)),
        Node {
            width: px(CARD_W),
            flex_direction: FlexDirection::Column,
            border_radius: BorderRadius::all(px(16.0)),
            border: UiRect::all(px(1.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(pal.bg_panel),
        BorderColor::all(pal.ink.with_alpha(0.16)),
        // Transparent until the crew is picked — `sync_selection` re-tints it.
        Outline::new(px(2.0), px(2.0), Color::NONE),
    ))
    .observe(deck_card_clicked)
    .with_children(|card| {
        // Faction accent strip.
        card.spawn((
            Node {
                width: percent(100.0),
                height: px(ACCENT_STRIP_H),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(visual.accent),
            Pickable::IGNORE,
        ));

        // Body (the captain art sits behind it, washed out).
        card.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(3.0),
                padding: UiRect::axes(px(13.0), px(11.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|body| {
            if let Some(handle) = art_handle {
                body.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0.0),
                        top: px(0.0),
                        width: percent(100.0),
                        height: percent(100.0),
                        ..default()
                    },
                    ImageNode::new(handle)
                        .with_color(Color::WHITE.with_alpha(0.30))
                        .with_mode(NodeImageMode::Stretch),
                    Pickable::IGNORE,
                ));
            }
            body.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    top: px(0.0),
                    width: percent(100.0),
                    height: percent(100.0),
                    ..default()
                },
                BackgroundGradient::from(LinearGradient::to_bottom(vec![
                    ColorStop::new(pal.bg_deep.with_alpha(0.55), percent(0.0)),
                    ColorStop::new(pal.bg_deep.with_alpha(0.94), percent(100.0)),
                ])),
                Pickable::IGNORE,
            ));

            // Name line: crest · crew · ✓.
            body.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(7.0),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|line| {
                spawn_crest(line, visual.crest, 18.0, visual.accent);
                line.spawn((
                    Text::new(visual.label),
                    title_font(fonts, 17.0),
                    TextColor(visual.accent),
                    Pickable::IGNORE,
                ));
                line.spawn((
                    DeckCardCheck { key, side },
                    Node {
                        display: Display::None,
                        width: px(18.0),
                        height: px(18.0),
                        margin: UiRect::left(Val::Auto),
                        border_radius: BorderRadius::MAX,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(visual.accent),
                    Pickable::IGNORE,
                    children![(
                        Text::new(CHECK),
                        TextFont {
                            font: check_font.clone().into(),
                            font_size: FontSize::Px(10.0),
                            ..default()
                        },
                        TextColor(pal.text_on_gold),
                        Pickable::IGNORE,
                    )],
                ));
            });

            body.spawn((
                Text::new(format!("Capitaine · {}", visual.captain)),
                label_font(fonts, 12.0),
                TextColor(pal.text.with_alpha(0.82)),
                Pickable::IGNORE,
            ));
            body.spawn((
                Text::new(visual.tagline),
                flavour_font(fonts, 11.0),
                TextColor(pal.text_faint),
                Pickable::IGNORE,
            ));
        });
    });
}

fn spawn_level_row(screen: &mut ChildSpawnerCommands, pal: &Palette, fonts: &Fonts) {
    screen
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(8.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|section| {
            section.spawn(caption(fonts, "Niveau de l'IA", pal.text_dim));
            section
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: px(12.0),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|row| {
                    for choice in LEVELS {
                        row.spawn((
                            LevelButton(choice.level),
                            Name::new(format!("Level({:?})", choice.level)),
                            Node {
                                width: px(LEVEL_W),
                                flex_direction: FlexDirection::Column,
                                row_gap: px(2.0),
                                padding: UiRect::axes(px(14.0), px(10.0)),
                                border: UiRect::all(px(2.0)),
                                border_radius: BorderRadius::all(px(12.0)),
                                ..default()
                            },
                            BackgroundColor(Color::WHITE.with_alpha(0.04)),
                            BorderColor::all(pal.ink.with_alpha(0.18)),
                        ))
                        .observe(level_clicked)
                        .with_children(|button| {
                            button.spawn((
                                LevelButtonLabel(choice.level),
                                Text::new(choice.label),
                                bold_label_font(fonts, 14.0),
                                TextColor(pal.text_dim),
                                Pickable::IGNORE,
                            ));
                            button.spawn((
                                Text::new(choice.sub),
                                flavour_font(fonts, 11.0),
                                TextColor(pal.text_faint),
                                Pickable::IGNORE,
                            ));
                        });
                    }
                });
        });
}

fn spawn_start_button(screen: &mut ChildSpawnerCommands, pal: &Palette, fonts: &Fonts) {
    screen
        .spawn((
            StartButton,
            Name::new("StartButton"),
            Node {
                width: px(START_W),
                height: px(START_H),
                margin: UiRect::top(px(4.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(px(1.0)),
                border_radius: BorderRadius::all(px(L::BUTTON_RADIUS)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(pal.ink.with_alpha(0.18)),
            BackgroundGradient::from(idle_cta(pal)),
        ))
        .observe(start_clicked)
        .with_children(|button| {
            button.spawn((
                StartButtonLabel,
                Text::new("Commencer le combat"),
                bold_label_font(fonts, 17.0),
                LetterSpacing::Px(0.5),
                TextColor(pal.text_faint),
                Pickable::IGNORE,
            ));
        });
}

/// TS `btn-ghost` — the CTA before both crews are picked.
fn idle_cta(pal: &Palette) -> LinearGradient {
    LinearGradient::to_bottom(vec![
        ColorStop::new(Color::WHITE.with_alpha(0.05), percent(0.0)),
        ColorStop::new(pal.bg_deep.with_alpha(0.30), percent(100.0)),
    ])
}

/// TS `btn-gold` — gold → amber, like the mock-up's `.btn`.
fn ready_cta(pal: &Palette) -> LinearGradient {
    LinearGradient::to_bottom_right(vec![
        ColorStop::new(pal.gold, percent(0.0)),
        ColorStop::new(pal.amber, percent(100.0)),
    ])
}

// ============================================================
// Interaction
// ============================================================

fn deck_card_clicked(
    click: On<Pointer<Click>>,
    cards: Query<&DeckCardButton>,
    mut selection: ResMut<SetupSelection>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let Ok(card) = cards.get(click.entity) else {
        return;
    };
    selection.pick(card.side, card.key);
}

fn level_clicked(
    click: On<Pointer<Click>>,
    buttons: Query<&LevelButton>,
    mut selection: ResMut<SetupSelection>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let Ok(button) = buttons.get(click.entity) else {
        return;
    };
    selection.level = button.0;
}

/// TS `startGame()` — the click only raises the intent, so the same path can
/// be exercised head-lessly (and later bound to a key).
fn start_clicked(click: On<Pointer<Click>>, mut requests: MessageWriter<StartGameRequested>) {
    if click.button != PointerButton::Primary {
        return;
    }
    requests.write(StartGameRequested);
}

/// Build the [`Session`] and enter the board — TS `router.push("/game?…")`.
// Bevy system params, one per resource this transition touches; a bundle would
// hide the same borrows behind an extra type.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_start_request(
    mut requests: MessageReader<StartGameRequested>,
    selection: Res<SetupSelection>,
    assets: Option<Res<AssetServer>>,
    mut art: ResMut<ArtCache>,
    mut commands: Commands,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut next: ResMut<NextState<AppScreen>>,
) {
    if requests.read().count() == 0 {
        return;
    }
    // TS `if (!ready) return;` — the disabled button.
    let Some((deck_you, deck_foe)) = selection.decks() else {
        return;
    };

    match Session::new(&deck_you, &deck_foe, selection.level, fresh_seed()) {
        Ok(session) => {
            // Nothing of the previous game may leak into the new one.
            *mode = UiMode::Idle;
            selected.0 = None;
            if let Some(assets) = assets.as_deref() {
                let ids = selection.art_def_ids();
                art.prewarm(assets, ids.iter().map(String::as_str));
            }
            commands.insert_resource(session);
            next.set(AppScreen::Board);
        }
        Err(error) => {
            // The four built-in decks are verified by the engine's own tests,
            // so this is a bug report rather than something to show the player.
            error!("could not create the game: {error}");
        }
    }
}

// ============================================================
// Reactive tinting
// ============================================================

/// Re-tint the cards, the difficulty buttons and the CTA after every change of
/// [`SetupSelection`] (and once on the first frame, so the initial
/// "Intermédiaire" highlight is drawn).
// One `Query` per widget family that has to be re-tinted; they are disjoint
// (hence the `Without` filters) and cannot be merged into a single query.
#[allow(clippy::too_many_arguments)]
pub(super) fn sync_selection(
    selection: Res<SetupSelection>,
    just_spawned: Query<(), Added<DeckCardButton>>,
    palette: Res<Palette>,
    mut cards: Query<(&DeckCardButton, &mut Outline, &mut BorderColor)>,
    mut checks: Query<(&DeckCardCheck, &mut Node)>,
    mut levels: Query<(&LevelButton, &mut BackgroundColor, &mut BorderColor), Without<DeckCardButton>>,
    mut level_labels: Query<(&LevelButtonLabel, &mut TextColor), Without<StartButtonLabel>>,
    mut start: Query<&mut BackgroundGradient, With<StartButton>>,
    mut start_label: Query<&mut TextColor, With<StartButtonLabel>>,
) {
    // `is_changed` covers every click; `just_spawned` covers the very first
    // frame, where the screen exists but nothing has been picked yet.
    if !selection.is_changed() && just_spawned.is_empty() {
        return;
    }
    let pal = *palette;

    for (card, mut outline, mut border) in &mut cards {
        let on = selection.picked(card.side) == Some(card.key);
        let accent = card.key.visual().accent;
        outline.color = if on { accent } else { Color::NONE };
        border.set_all(if on {
            accent
        } else {
            pal.ink.with_alpha(0.16)
        });
    }

    for (check, mut node) in &mut checks {
        let on = selection.picked(check.side) == Some(check.key);
        node.display = if on { Display::Flex } else { Display::None };
    }

    for (button, mut background, mut border) in &mut levels {
        let on = selection.level == button.0;
        background.0 = if on {
            Color::WHITE.with_alpha(0.10)
        } else {
            Color::WHITE.with_alpha(0.04)
        };
        border.set_all(if on { pal.gold } else { pal.ink.with_alpha(0.18) });
    }

    for (label, mut color) in &mut level_labels {
        color.0 = if selection.level == label.0 {
            pal.gold
        } else {
            pal.text_dim
        };
    }

    let ready = selection.is_ready();
    for mut gradient in &mut start {
        *gradient = BackgroundGradient::from(if ready {
            ready_cta(&pal)
        } else {
            idle_cta(&pal)
        });
    }
    for mut color in &mut start_label {
        color.0 = if ready { pal.text_on_gold } else { pal.text_faint };
    }
}

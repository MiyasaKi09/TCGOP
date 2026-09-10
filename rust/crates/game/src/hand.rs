//! The fanned hand and the footer that sits under it.
//!
//! Behaviour reference: the hand block, the "Actions" column and the log of
//! `src/components/Game.tsx`, plus `src/components/Card.tsx` /
//! `src/components/FullCard.tsx` for what a card shows. Visual reference: the
//! `footer` / `.hand` rules of `public/tests/board-hs-anime-v2.html`.
//!
//! Layout, top to bottom:
//!
//! ```text
//! ┌───────────────────────────────────────────┬──────────────┐
//! │ MAIN 5                                    │ Deck 41      │
//! │      ╱‾╲  ╱‾╲  ╱‾╲  ╱‾╲  ╱‾╲              │ ⚔ hints…     │
//! │      │  │ │  │ │  │ │  │ │  │  ← the fan  │ [Fin de tour]│
//! │      ╰──╯ ╰──╯ ╰──╯ ╰──╯ ╰──╯             │ [✕ Annuler]  │
//! ├───────────────────────────────────────────┴──────────────┤
//! │ T4 ► Zoro attaque Coby …                      (the log)  │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! Everything the hand *decides* is a pure function ([`hand_click_command`],
//! [`hand_drag_command`], [`footer_hints`], [`log_lines`] and the geometry in
//! [`layout`]); the systems only turn those answers into nodes, so the whole
//! interaction is unit-testable under `MinimalPlugins`.
//!
//! Rendering systems are gated on `resource_exists::<AssetServer>`, so a
//! head-less `App` runs the logic without ever touching the UI tree.

pub mod card_face;
pub mod layout;

use bevy::prelude::*;
use bevy::text::LetterSpacing;
use bevy::ui::widget::Text;
use bevy::window::PrimaryWindow;
use tcgop_engine::state::LogEntry;
use tcgop_engine::types::{CardType, GameAction, HakiType, PlayerId};

use crate::app::{AppScreen, AppSet, Fonts, Palette, configure_pipeline, layout as L};
use crate::art::ArtCache;
use crate::bridge::{BridgeSet, DispatchAction, Session};
use crate::selection::{
    SelectedHandCard, UiCommand, UiMode, hand_card_playable, on_hand_card_click,
};

use self::card_face::{
    ANCHOR, ARROW_RIGHT, ArtCover, CARET_LEFT, CARET_RIGHT, CROSS, CROSSED_SWORDS, CardFace,
    FaceCtx, STAR, spawn_card_face,
};
use self::layout::{HandCardState, cover_fit, fan_gap, fan_transform, preview_placement};

// ============================================================
// Plugin
// ============================================================

/// Fanned hand, hover preview, action column and rolling log.
pub struct HandPlugin;

impl Plugin for HandPlugin {
    fn build(&self, app: &mut App) {
        let symbols = match app.world().get_resource::<AssetServer>() {
            Some(assets) => SymbolFont(assets.load(SYMBOL_FONT_PATH)),
            None => SymbolFont::default(),
        };

        configure_pipeline(app);
        app.insert_resource(symbols)
            .init_resource::<HoveredHandCard>()
            .init_resource::<HandView>()
            .init_resource::<LogView>()
            .add_message::<HandCardClicked>()
            .add_message::<HandCardDragStarted>()
            // Logic: pure, head-less, and enqueued *before* the bridge so a
            // click reaches the engine in the same frame.
            .add_systems(
                Update,
                (handle_hand_clicks, handle_hand_drags)
                    .chain()
                    .in_set(AppSet::Selection)
                    .before(BridgeSet)
                    .run_if(resource_exists::<Session>),
            )
            // Presentation: only when there is something to render into.
            .add_systems(
                OnEnter(AppScreen::Board),
                spawn_footer.run_if(resource_exists::<AssetServer>),
            )
            .add_systems(
                Update,
                (
                    rebuild_hand,
                    (
                        fit_fan_width,
                        apply_fan_transforms,
                        update_deck_count,
                        update_footer_hints,
                        update_footer_buttons,
                        update_log,
                        update_preview,
                    ),
                    (sync_art_cover, pulse_playable_pips),
                )
                    .chain()
                    .in_set(AppSet::Render)
                    .after(BridgeSet)
                    .run_if(in_state(AppScreen::Board))
                    .run_if(resource_exists::<Session>)
                    .run_if(resource_exists::<AssetServer>),
            );
    }
}

/// The only font in `assets/fonts` with symbol coverage — Cinzel, Oswald,
/// Spectral and Bangers stop at Latin-1, so every ⚔ / ★ / ♥ / ➡ goes through
/// this handle. See [`card_face`] for the glyph constants.
pub const SYMBOL_FONT_PATH: &str = "fonts/DejaVuSans.ttf";

/// Handle for [`SYMBOL_FONT_PATH`]. Falls back to `Handle::default()` head-lessly.
#[derive(Resource, Debug, Clone, Default)]
pub struct SymbolFont(pub Handle<Font>);

// ============================================================
// Messages
// ============================================================

/// A hand card was clicked (TS `Card.onClick` in the hand loop).
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct HandCardClicked(pub String);

/// A drag started on a hand card (TS `handleHandDragStart`).
///
/// The board can finish the gesture by observing `Pointer<DragDrop>` on its
/// cells and routing it through
/// [`on_cell_click`](crate::selection::on_cell_click): by the time the pointer
/// is over a cell the [`UiMode`] is already `SelectingSlot` /
/// `SelectingEquipTarget`, exactly as if the card had been clicked.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct HandCardDragStarted(pub String);

// ============================================================
// Resources
// ============================================================

/// TS `hoveredHand` — the instance id under the pointer, if any.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct HoveredHandCard(pub Option<String>);

/// What the fan was last built from, so it is only rebuilt when it changed.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
struct HandView {
    /// `(instance_id, playable)` for every card, in hand order.
    key: Vec<(String, bool)>,
    built: bool,
}

/// How long `state.log` was when the log panel was last rebuilt.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LogView {
    len: usize,
    built: bool,
}

// ============================================================
// Components
// ============================================================

/// One card of the fan (the wrapper that carries the fan transform).
#[derive(Component, Debug, Clone)]
pub struct HandCard {
    pub instance_id: String,
    /// Position in the hand, left to right.
    pub index: usize,
    /// The card frame inside the wrapper (its border shows the selection).
    pub face: Entity,
}

/// The row the fan lives in.
#[derive(Component)]
struct HandCardsRow;

/// The "MAIN <n>" counter.
#[derive(Component)]
struct HandCountText;

/// The "Deck <n>" counter.
#[derive(Component)]
struct DeckCountText;

/// The amber hint box and its three lines, as one component so a single query
/// can toggle all of them without disjointness filters.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum HintNode {
    /// The box: shown when any line is.
    Box,
    /// `⚔ Clique ton Capitaine pour l'engager`.
    Flip,
    /// `★ Haki des Rois dispo (Capitaine)`.
    KingHaki,
    /// `⚓ Clique ton Navire pour l'activer`.
    Ship,
}

/// The gold "Fin de tour ➡" button. The flag is what the button currently
/// *looks* like, so the gradient is only rewritten when it actually changes.
#[derive(Component)]
struct EndTurnButton {
    disabled: bool,
}

/// The ghost "✕ Annuler" button (only while the mode is not idle).
#[derive(Component)]
struct CancelButton;

/// The rolling log panel.
#[derive(Component)]
struct LogPanel;

/// The floating full-size preview above the hovered card.
#[derive(Component)]
struct HandPreview(String);

/// The green "playable" pip under a card.
#[derive(Component)]
struct PlayablePip;

// ============================================================
// Pure decisions
// ============================================================

/// TS, in the hand loop:
///
/// ```ts
/// onClick={() => {
///   if (canPlay) handleHandCardClick(id);
///   else setUiMode({ type: "cardDetail", defId: card.defId, instanceId: id });
/// }}
/// ```
pub fn hand_click_command(
    playable: bool,
    card_type: CardType,
    instance_id: &str,
    def_id: &str,
) -> UiCommand {
    if playable {
        on_hand_card_click(card_type, instance_id)
    } else {
        UiCommand::SetMode(UiMode::CardDetail {
            def_id: def_id.to_string(),
            instance_id: Some(instance_id.to_string()),
        })
    }
}

/// TS `handleHandDragStart` — only characters and objects are drag sources, and
/// only while `draggable={canPlay && dragType}` holds.
pub fn hand_drag_command(playable: bool, card_type: CardType, instance_id: &str) -> UiCommand {
    if !playable {
        return UiCommand::Ignore;
    }
    match card_type {
        CardType::Character | CardType::Object => on_hand_card_click(card_type, instance_id),
        CardType::Ship | CardType::Event | CardType::Counter => UiCommand::Ignore,
    }
}

/// TS `canFlip` / `canKingHaki` / `canActivateShip`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FooterHints {
    pub can_flip: bool,
    pub can_king_haki: bool,
    pub can_activate_ship: bool,
}

impl FooterHints {
    pub fn any(&self) -> bool {
        self.can_flip || self.can_king_haki || self.can_activate_ship
    }

    fn shows(&self, node: HintNode) -> bool {
        match node {
            HintNode::Box => self.any(),
            HintNode::Flip => self.can_flip,
            HintNode::KingHaki => self.can_king_haki,
            HintNode::Ship => self.can_activate_ship,
        }
    }
}

/// Derive the three hints from the human's legal actions.
pub fn footer_hints(valid: &[GameAction]) -> FooterHints {
    FooterHints {
        can_flip: valid
            .iter()
            .any(|a| matches!(a, GameAction::FlipCaptain { .. })),
        can_king_haki: valid.iter().any(|a| {
            matches!(
                a,
                GameAction::UseHaki {
                    haki_type: HakiType::King,
                    ..
                }
            )
        }),
        can_activate_ship: valid
            .iter()
            .any(|a| matches!(a, GameAction::ActivateShip { .. })),
    }
}

/// TS `state.log.slice(-12).reverse()` — newest first.
pub const LOG_VISIBLE: usize = 12;

/// One rendered log row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub turn: u32,
    pub is_you: bool,
    pub message: String,
    /// The first (newest) row is drawn brighter, like the web.
    pub newest: bool,
}

/// TS `state.log.slice(-12).reverse().map(…)`.
pub fn log_lines(log: &[LogEntry], human: PlayerId) -> Vec<LogLine> {
    let from = log.len().saturating_sub(LOG_VISIBLE);
    log[from..]
        .iter()
        .rev()
        .enumerate()
        .map(|(i, entry)| LogLine {
            turn: entry.turn,
            is_you: entry.player == human,
            message: entry.message.clone(),
            newest: i == 0,
        })
        .collect()
}

/// TS `resetUI()` plus the dispatch, in one place: every `Dispatch` command
/// both sends the action *and* returns the UI to idle.
fn apply_ui_command(
    command: UiCommand,
    mode: &mut UiMode,
    selected: &mut SelectedHandCard,
    dispatch: &mut MessageWriter<DispatchAction>,
) {
    match command {
        UiCommand::Ignore => {}
        UiCommand::Dispatch(action) => {
            dispatch.write(DispatchAction(action));
            *mode = UiMode::Idle;
            selected.0 = None;
        }
        UiCommand::SetMode(next) => *mode = next,
        UiCommand::SelectHandCard { instance_id, mode: next } => {
            selected.0 = Some(instance_id);
            *mode = next;
        }
        UiCommand::Reset => {
            *mode = UiMode::Idle;
            selected.0 = None;
        }
    }
}

// ============================================================
// Logic systems (head-less)
// ============================================================

fn handle_hand_clicks(
    mut clicks: MessageReader<HandCardClicked>,
    session: Res<Session>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut dispatch: MessageWriter<DispatchAction>,
) {
    for HandCardClicked(instance_id) in clicks.read() {
        let Some(card) = session.state.card(instance_id) else {
            continue;
        };
        let Some(def) = session.registry.card_def(&card.def_id) else {
            continue;
        };
        let playable = hand_card_playable(&session.valid, instance_id);
        let command = hand_click_command(playable, def.card_type, instance_id, &card.def_id);
        apply_ui_command(command, &mut mode, &mut selected, &mut dispatch);
    }
}

fn handle_hand_drags(
    mut drags: MessageReader<HandCardDragStarted>,
    session: Res<Session>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut dispatch: MessageWriter<DispatchAction>,
) {
    for HandCardDragStarted(instance_id) in drags.read() {
        let Some(card) = session.state.card(instance_id) else {
            continue;
        };
        let Some(def) = session.registry.card_def(&card.def_id) else {
            continue;
        };
        let playable = hand_card_playable(&session.valid, instance_id);
        let command = hand_drag_command(playable, def.card_type, instance_id);
        apply_ui_command(command, &mut mode, &mut selected, &mut dispatch);
    }
}

// ============================================================
// Footer skeleton
// ============================================================

fn label_font(fonts: &Fonts, size: f32) -> TextFont {
    TextFont {
        font: fonts.oswald.clone().into(),
        font_size: FontSize::Px(size),
        ..default()
    }
}

fn bold_label_font(fonts: &Fonts, size: f32) -> TextFont {
    TextFont {
        font: fonts.oswald_bold.clone().into(),
        font_size: FontSize::Px(size),
        ..default()
    }
}

fn symbol_font(symbols: &SymbolFont, size: f32) -> TextFont {
    TextFont {
        font: symbols.0.clone().into(),
        font_size: FontSize::Px(size),
        ..default()
    }
}

fn spawn_footer(
    mut commands: Commands,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    symbols: Res<SymbolFont>,
    mut hand_view: ResMut<HandView>,
    mut log_view: ResMut<LogView>,
) {
    // The panels are respawned from scratch, so the "already drawn" caches
    // must forget the previous game.
    *hand_view = HandView::default();
    *log_view = LogView::default();
    let pal = *palette;

    commands
        .spawn((
            Name::new("HandFooter"),
            Node {
                position_type: PositionType::Absolute,
                left: px(0.),
                right: px(0.),
                bottom: px(0.),
                flex_direction: FlexDirection::Column,
                row_gap: px(6.),
                padding: UiRect {
                    left: px(L::HAND_PAD_X),
                    right: px(L::HAND_PAD_X),
                    top: px(6.),
                    bottom: px(10.),
                },
                ..default()
            },
            BackgroundGradient::from(LinearGradient::to_top(vec![
                ColorStop::new(pal.bg_deep.with_alpha(0.97), percent(0.)),
                ColorStop::new(Color::NONE, percent(100.)),
            ])),
            GlobalZIndex(L::Z_HAND),
            DespawnOnExit(AppScreen::Board),
            Pickable {
                should_block_lower: true,
                is_hoverable: false,
            },
        ))
        .with_children(|footer| {
            footer
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::End,
                    column_gap: px(12.),
                    ..default()
                })
                .with_children(|row| {
                    spawn_hand_column(row, &pal, &fonts);
                    spawn_action_column(row, &pal, &fonts, &symbols);
                });
            spawn_log_panel(footer, &pal);
        });
}

fn spawn_hand_column(row: &mut ChildSpawnerCommands, pal: &Palette, fonts: &Fonts) {
    row.spawn(Node {
        flex_grow: 1.,
        flex_shrink: 1.,
        min_width: px(0.),
        flex_direction: FlexDirection::Column,
        ..default()
    })
    .with_children(|col| {
        // "MAIN 5 ──────"
        col.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(8.),
            margin: UiRect::bottom(px(4.)),
            ..default()
        })
        .with_children(|head| {
            head.spawn((
                Text::new("MAIN"),
                TextFont {
                    font: fonts.oswald.clone().into(),
                    font_size: FontSize::Px(L::FS_LABEL),
                    ..default()
                },
                LetterSpacing::Px(1.6),
                TextColor(pal.text_faint),
            ));
            head.spawn((
                HandCountText,
                Text::new("0"),
                bold_label_font(fonts, L::FS_STAT),
                TextColor(pal.text_dim),
            ));
            head.spawn((
                Node {
                    flex_grow: 1.,
                    height: px(1.),
                    ..default()
                },
                BackgroundColor(pal.ink.with_alpha(0.10)),
            ));
        });

        // The fan itself.
        col.spawn((
            HandCardsRow,
            Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::End,
                column_gap: px(L::HAND_GAP),
                // The outer cards drop by up to `HAND_FAN_LIFT_MAX`, so the row
                // reserves that much under them.
                height: px(L::HAND_CARD_H + L::HAND_FAN_LIFT_MAX),
                padding: UiRect::bottom(px(L::HAND_FAN_LIFT_MAX)),
                ..default()
            },
        ));
    });
}

fn spawn_action_column(
    row: &mut ChildSpawnerCommands,
    pal: &Palette,
    fonts: &Fonts,
    symbols: &SymbolFont,
) {
    row.spawn(Node {
        width: px(L::ACTION_COL_W),
        flex_shrink: 0.,
        flex_direction: FlexDirection::Column,
        row_gap: px(6.),
        ..default()
    })
    .with_children(|col| {
        // Deck count.
        col.spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            padding: UiRect::horizontal(px(4.)),
            ..default()
        })
        .with_children(|line| {
            line.spawn((
                Text::new("Deck"),
                label_font(fonts, L::FS_LABEL),
                TextColor(pal.text_faint),
            ));
            line.spawn((
                DeckCountText,
                Text::new("0"),
                bold_label_font(fonts, L::FS_LABEL),
                TextColor(pal.text_dim),
            ));
        });

        // Amber hint box.
        col.spawn((
            HintNode::Box,
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(2.),
                padding: UiRect::axes(px(8.), px(6.)),
                border: UiRect::all(px(1.)),
                border_radius: BorderRadius::all(px(10.)),
                ..default()
            },
            BackgroundColor(pal.gold.with_alpha(0.12)),
            BorderColor::all(pal.gold.with_alpha(0.30)),
        ))
        .with_children(|hints| {
            for (line, glyph, text) in [
                (
                    HintNode::Flip,
                    CROSSED_SWORDS,
                    "Clique ton Capitaine pour l'engager",
                ),
                (HintNode::KingHaki, STAR, "Haki des Rois dispo (Capitaine)"),
                (HintNode::Ship, ANCHOR, "Clique ton Navire pour l'activer"),
            ] {
                hints
                    .spawn((
                        line,
                        Node {
                            display: Display::None,
                            flex_direction: FlexDirection::Row,
                            column_gap: px(4.),
                            align_items: AlignItems::Start,
                            ..default()
                        },
                    ))
                    .with_children(|entry| {
                        entry.spawn((
                            Text::new(glyph),
                            symbol_font(symbols, 9.),
                            TextColor(pal.amber),
                        ));
                        entry.spawn((
                            Text::new(text),
                            label_font(fonts, 9.),
                            TextColor(pal.amber),
                        ));
                    });
            }
        });

        // "Fin de tour ➡"
        col.spawn((
            EndTurnButton { disabled: false },
            Button,
            Node {
                height: px(L::BUTTON_H),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: px(6.),
                border_radius: BorderRadius::all(px(L::BUTTON_RADIUS)),
                ..default()
            },
            end_turn_gradient(pal, false),
            children![
                (
                    Text::new("Fin de tour"),
                    bold_label_font(fonts, 14.),
                    TextColor(pal.text_on_gold),
                    Pickable::IGNORE,
                ),
                (
                    Text::new(ARROW_RIGHT),
                    symbol_font(symbols, 12.),
                    TextColor(pal.text_on_gold),
                    Pickable::IGNORE,
                ),
            ],
        ))
        .observe(end_turn_clicked);

        // "✕ Annuler"
        col.spawn((
            CancelButton,
            Button,
            Node {
                display: Display::None,
                height: px(26.),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: px(5.),
                border: UiRect::all(px(1.)),
                border_radius: BorderRadius::all(px(10.)),
                ..default()
            },
            BorderColor::all(pal.ink),
            children![
                (
                    Text::new(CROSS),
                    symbol_font(symbols, 9.),
                    TextColor(pal.text_dim),
                    Pickable::IGNORE,
                ),
                (
                    Text::new("Annuler"),
                    label_font(fonts, 11.),
                    TextColor(pal.text_dim),
                    Pickable::IGNORE,
                ),
            ],
        ))
        .observe(cancel_clicked);
    });
}

fn spawn_log_panel(footer: &mut ChildSpawnerCommands, pal: &Palette) {
    footer.spawn((
        LogPanel,
        Node {
            height: px(L::LOG_H),
            overflow: Overflow::clip(),
            flex_direction: FlexDirection::Column,
            row_gap: px(1.),
            padding: UiRect::axes(px(10.), px(5.)),
            border: UiRect::all(px(1.)),
            border_radius: BorderRadius::all(px(10.)),
            ..default()
        },
        BackgroundColor(pal.bg_deep.with_alpha(0.70)),
        BorderColor::all(pal.ink.with_alpha(0.12)),
        Pickable::IGNORE,
    ));
}

// ============================================================
// Observers
// ============================================================

fn end_turn_clicked(
    click: On<Pointer<Click>>,
    session: Option<Res<Session>>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut dispatch: MessageWriter<DispatchAction>,
) {
    let _ = click;
    // TS `disabled={isAiTurn || inCounterWindow}`.
    let Some(session) = session else { return };
    if session.is_ai_turn() || session.in_counter_window() {
        return;
    }
    apply_ui_command(
        UiCommand::Dispatch(GameAction::EndTurn),
        &mut mode,
        &mut selected,
        &mut dispatch,
    );
}

fn cancel_clicked(
    click: On<Pointer<Click>>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
) {
    let _ = click;
    *mode = UiMode::Idle;
    selected.0 = None;
}

fn hand_card_entered(
    event: On<Pointer<Enter>>,
    cards: Query<&HandCard>,
    mut hovered: ResMut<HoveredHandCard>,
) {
    if let Ok(card) = cards.get(event.entity) {
        hovered.0 = Some(card.instance_id.clone());
    }
}

fn hand_card_left(
    event: On<Pointer<Leave>>,
    cards: Query<&HandCard>,
    mut hovered: ResMut<HoveredHandCard>,
) {
    if let Ok(card) = cards.get(event.entity)
        && hovered.0.as_deref() == Some(card.instance_id.as_str())
    {
        hovered.0 = None;
    }
}

fn hand_card_clicked(
    event: On<Pointer<Click>>,
    cards: Query<&HandCard>,
    mut clicks: MessageWriter<HandCardClicked>,
) {
    if let Ok(card) = cards.get(event.entity) {
        clicks.write(HandCardClicked(card.instance_id.clone()));
    }
}

fn hand_card_drag_started(
    event: On<Pointer<DragStart>>,
    cards: Query<&HandCard>,
    mut hovered: ResMut<HoveredHandCard>,
    mut drags: MessageWriter<HandCardDragStarted>,
) {
    if let Ok(card) = cards.get(event.entity) {
        // TS `onDragStart` clears the hover preview first.
        hovered.0 = None;
        drags.write(HandCardDragStarted(card.instance_id.clone()));
    }
}

// ============================================================
// Rendering systems
// ============================================================

/// Rebuild the fan whenever the hand's contents or their playability changed.
#[allow(clippy::too_many_arguments)]
fn rebuild_hand(
    mut commands: Commands,
    session: Res<Session>,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    symbols: Res<SymbolFont>,
    assets: Res<AssetServer>,
    mut art: ResMut<ArtCache>,
    mut view: ResMut<HandView>,
    mut hovered: ResMut<HoveredHandCard>,
    row: Query<Entity, With<HandCardsRow>>,
    mut count_text: Query<&mut Text, With<HandCountText>>,
) {
    let Ok(row) = row.single() else {
        return;
    };
    let hand = &session.you().hand;
    let key: Vec<(String, bool)> = hand
        .iter()
        .map(|id| (id.clone(), hand_card_playable(&session.valid, id)))
        .collect();
    if view.built && view.key == key {
        return;
    }
    view.key = key;
    view.built = true;

    // The card under the pointer may have just been played: drop the stale
    // hover so the floating preview does not linger on a card that is gone.
    if let Some(id) = hovered.0.clone()
        && !hand.iter().any(|card| card == &id)
    {
        hovered.0 = None;
    }

    if let Ok(mut text) = count_text.single_mut() {
        text.0 = hand.len().to_string();
    }

    commands.entity(row).despawn_related::<Children>();

    let pal = *palette;
    let count = hand.len();
    let entries: Vec<(String, bool)> = view.key.clone();
    let mut faces: Vec<(usize, String, bool, Option<Handle<Image>>)> = Vec::with_capacity(count);
    for (index, (instance_id, playable)) in entries.iter().enumerate() {
        let Some(card) = session.state.card(instance_id) else {
            continue;
        };
        let handle = art.card(
            &assets,
            &card.def_id,
            card.is_awakened.unwrap_or(false),
        );
        faces.push((index, instance_id.clone(), *playable, handle));
    }

    commands.entity(row).with_children(|row| {
        if faces.is_empty() {
            row.spawn((
                Text::new("Main vide"),
                TextFont {
                    font: fonts.spectral.clone().into(),
                    font_size: FontSize::Px(13.),
                    ..default()
                },
                TextColor(pal.text_faint),
                Pickable::IGNORE,
            ));
            return;
        }

        for (index, instance_id, playable, handle) in faces {
            let Some(card) = session.state.card(&instance_id) else {
                continue;
            };
            let Some(def) = session.registry.card_def(&card.def_id) else {
                continue;
            };
            let face = CardFace::from_def(def, Some(card), L::HAND_CARD_W);
            let ctx = FaceCtx {
                palette: &pal,
                fonts: &fonts,
                symbols: &symbols.0,
                art: handle,
            };

            let mut wrapper = row.spawn((
                Node {
                    width: px(L::HAND_CARD_W),
                    height: px(L::HAND_CARD_H),
                    flex_shrink: 0.,
                    ..default()
                },
                UiTransform::IDENTITY,
            ));
            let mut face_entity = Entity::PLACEHOLDER;
            wrapper.with_children(|w| {
                face_entity = spawn_card_face(w, &ctx, &face);
                if playable {
                    w.spawn((
                        PlayablePip,
                        Node {
                            position_type: PositionType::Absolute,
                            bottom: px(-2.),
                            left: percent(50.),
                            width: px(40.),
                            height: px(4.),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        UiTransform::from_translation(Val2::px(-20., 0.)),
                        BackgroundColor(pal.deploy),
                        Pickable::IGNORE,
                    ));
                }
            });
            wrapper.insert(HandCard {
                instance_id,
                index,
                face: face_entity,
            });
            wrapper
                .observe(hand_card_entered)
                .observe(hand_card_left)
                .observe(hand_card_clicked)
                .observe(hand_card_drag_started);
        }
    });
}

/// The fan (rotation + lift) and the gold frame of the selected card.
fn apply_fan_transforms(
    hovered: Res<HoveredHandCard>,
    selected: Res<SelectedHandCard>,
    palette: Res<Palette>,
    cards: Query<(&HandCard, &mut UiTransform)>,
    mut borders: Query<&mut BorderColor>,
) {
    let count = cards.iter().count();
    for (card, mut transform) in cards {
        let state = HandCardState {
            hovered: hovered.0.as_deref() == Some(card.instance_id.as_str()),
            selected: selected.0.as_deref() == Some(card.instance_id.as_str()),
        };
        let fan = fan_transform(card.index, count, state, L::HAND_CARD_H);
        *transform = UiTransform {
            translation: Val2::px(fan.offset_x, fan.offset_y),
            scale: Vec2::splat(fan.scale),
            rotation: Rot2::degrees(fan.rotation_deg),
        };
        if let Ok(mut border) = borders.get_mut(card.face) {
            if state.selected {
                border.set_all(palette.gold);
            } else if state.hovered {
                border.set_all(palette.gold.with_alpha(0.55));
            } else {
                border.set_all(palette.ink);
            }
        }
    }
}

/// TS "Deck <n>" in the actions column.
fn update_deck_count(session: Res<Session>, mut deck: Query<&mut Text, With<DeckCountText>>) {
    if let Ok(mut text) = deck.single_mut() {
        text.0 = session.you().deck.len().to_string();
    }
}

/// The amber hint box — TS `canFlip` / `canKingHaki` / `canActivateShip`.
fn update_footer_hints(session: Res<Session>, mut hints: Query<(&HintNode, &mut Node)>) {
    let shown = footer_hints(&session.valid);
    for (node, mut style) in &mut hints {
        style.display = if shown.shows(*node) {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// "Fin de tour" availability and the "✕ Annuler" escape hatch.
fn update_footer_buttons(
    session: Res<Session>,
    mode: Res<UiMode>,
    palette: Res<Palette>,
    mut cancel: Query<&mut Node, With<CancelButton>>,
    mut end_turn: Query<(&mut EndTurnButton, &mut BackgroundGradient)>,
) {
    // TS `{uiMode.type !== "idle" && <button …>✕ Annuler</button>}`.
    if let Ok(mut node) = cancel.single_mut() {
        node.display = if mode.is_idle() {
            Display::None
        } else {
            Display::Flex
        };
    }

    // TS `disabled={isAiTurn || inCounterWindow}` — the gold gradient goes flat
    // and grey while the button is dead.
    let disabled = session.is_ai_turn() || session.in_counter_window();
    if let Ok((mut button, mut gradient)) = end_turn.single_mut()
        && button.disabled != disabled
    {
        button.disabled = disabled;
        *gradient = end_turn_gradient(&palette, disabled);
    }
}

/// The "Fin de tour" fill: gold → amber when live, flat slate when disabled.
fn end_turn_gradient(palette: &Palette, disabled: bool) -> BackgroundGradient {
    let (from, to) = if disabled {
        (
            palette.bg_slot.with_alpha(0.85),
            palette.bg_slot.with_alpha(0.85),
        )
    } else {
        (palette.gold, palette.amber)
    };
    BackgroundGradient::from(LinearGradient::to_bottom_right(vec![
        ColorStop::new(from, percent(0.)),
        ColorStop::new(to, percent(100.)),
    ]))
}

/// Keep the fan inside the window: the row's own width decides the gap, which
/// goes negative (a real overlapping fan) once the hand no longer fits.
fn fit_fan_width(
    mut row: Query<(&ComputedNode, &mut Node), With<HandCardsRow>>,
    mut cards: Query<(&HandCard, &mut Node), Without<HandCardsRow>>,
) {
    let Ok((computed, mut row_node)) = row.single_mut() else {
        return;
    };
    let available = computed.size().x * computed.inverse_scale_factor();
    if available <= 1.0 {
        // The first frame, before layout has run.
        return;
    }
    let count = cards.iter().count();
    let gap = fan_gap(count, available);
    row_node.column_gap = px(gap.max(0.0));
    // Taffy has no negative `gap`, so an overlapping fan is expressed as a
    // negative left margin on every card but the first.
    let overlap = gap.min(0.0);
    for (card, mut node) in &mut cards {
        node.margin.left = px(if card.index == 0 { 0.0 } else { overlap });
    }
}

/// Rebuild the rolling log when the engine appended to it.
fn update_log(
    mut commands: Commands,
    session: Res<Session>,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    symbols: Res<SymbolFont>,
    mut view: ResMut<LogView>,
    panel: Query<Entity, With<LogPanel>>,
) {
    let Ok(panel) = panel.single() else {
        return;
    };
    let len = session.state.log.len();
    if view.built && view.len == len {
        return;
    }
    view.len = len;
    view.built = true;

    let lines = log_lines(&session.state.log, session.human);
    let pal = *palette;
    commands.entity(panel).despawn_related::<Children>();
    commands.entity(panel).with_children(|panel| {
        for line in lines {
            let tint = if line.newest {
                pal.text.with_alpha(0.85)
            } else {
                pal.text.with_alpha(0.45)
            };
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: px(5.),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new(format!("T{}", line.turn)),
                        label_font(&fonts, L::FS_TINY),
                        TextColor(pal.text_faint),
                    ));
                    row.spawn((
                        Text::new(if line.is_you { CARET_RIGHT } else { CARET_LEFT }),
                        symbol_font(&symbols, 8.),
                        TextColor(if line.is_you { pal.hp } else { pal.foe }),
                    ));
                    row.spawn((
                        Text::new(line.message.clone()),
                        TextFont {
                            font: fonts.spectral.clone().into(),
                            font_size: FontSize::Px(L::FS_BODY),
                            ..default()
                        },
                        TextColor(tint),
                    ));
                });
        }
    });
}

/// The big floating card above the hovered hand card — TS "Hand hover preview".
#[allow(clippy::too_many_arguments)]
fn update_preview(
    mut commands: Commands,
    session: Res<Session>,
    mode: Res<UiMode>,
    hovered: Res<HoveredHandCard>,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    symbols: Res<SymbolFont>,
    assets: Res<AssetServer>,
    mut art: ResMut<ArtCache>,
    cards: Query<(&HandCard, &ComputedNode, &UiGlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    existing: Query<(Entity, &HandPreview)>,
) {
    let wanted = hovered
        .0
        .as_ref()
        .filter(|_| mode.is_idle() && !session.in_counter_window());

    let Some(instance_id) = wanted else {
        for (entity, _) in &existing {
            commands.entity(entity).despawn();
        }
        return;
    };

    if let Ok((_, shown)) = existing.single()
        && &shown.0 == instance_id
    {
        return;
    }
    for (entity, _) in &existing {
        commands.entity(entity).despawn();
    }

    let Some((_, computed, transform)) = cards
        .iter()
        .find(|(card, _, _)| &card.instance_id == instance_id)
    else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };
    let inv = computed.inverse_scale_factor();
    let size = computed.size() * inv;
    let centre = transform.translation * inv;
    let placement = preview_placement(
        centre.x - size.x / 2.0,
        centre.y - size.y / 2.0,
        size.x,
        centre.y + size.y / 2.0,
        window.width(),
    );

    let Some(card) = session.state.card(instance_id) else {
        return;
    };
    let Some(def) = session.registry.card_def(&card.def_id) else {
        return;
    };
    let handle = art.card(&assets, &card.def_id, card.is_awakened.unwrap_or(false));
    let pal = *palette;
    let face = CardFace::from_def(def, Some(card), L::FULL_CARD_W);
    let ctx = FaceCtx {
        palette: &pal,
        fonts: &fonts,
        symbols: &symbols.0,
        art: handle,
    };

    commands
        .spawn((
            HandPreview(instance_id.clone()),
            Name::new("HandPreview"),
            Node {
                position_type: PositionType::Absolute,
                left: px(placement.left),
                top: px(placement.top),
                ..default()
            },
            GlobalZIndex(L::Z_POPOVER),
            DespawnOnExit(AppScreen::Board),
            Pickable::IGNORE,
        ))
        .with_children(|preview| {
            spawn_card_face(preview, &ctx, &face);
        });
}

/// Re-crop an illustration once its real pixel size is known — the initial
/// crop assumes the nominal 700x1050 of `assets/cards`.
fn sync_art_cover(
    images: Res<Assets<Image>>,
    covers: Query<(&ImageNode, &mut ArtCover, &mut Node)>,
) {
    for (image_node, mut cover, mut node) in covers {
        let Some(image) = images.get(&image_node.image) else {
            continue;
        };
        let (w, h) = (image.width() as f32, image.height() as f32);
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        let aspect = h / w;
        if (aspect - cover.applied_aspect).abs() < 1e-3 {
            continue;
        }
        cover.applied_aspect = aspect;
        let fit = cover_fit(cover.box_w, cover.box_h, aspect, cover.focus);
        node.left = px(fit.left);
        node.top = px(fit.top);
        node.width = px(fit.width);
        node.height = px(fit.height);
    }
}

/// TS `animate-pulse` on the green playable pip.
fn pulse_playable_pips(
    time: Res<Time>,
    palette: Res<Palette>,
    pips: Query<&mut BackgroundColor, With<PlayablePip>>,
) {
    let t = time.elapsed_secs() * 3.0;
    let alpha = 0.55 + 0.45 * (t.sin() * 0.5 + 0.5);
    for mut background in pips {
        background.0 = palette.deploy.with_alpha(alpha);
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::BridgePlugin;
    use crate::bridge::testkit::{advance, session as make_session};
    use crate::selection::SelectionPlugin;
    use tcgop_engine::types::Slot;

    // --- click routing ---------------------------------------

    #[test]
    fn a_playable_character_asks_for_a_slot() {
        assert_eq!(
            hand_click_command(true, CardType::Character, "c1", "MG-001"),
            UiCommand::SelectHandCard {
                instance_id: "c1".into(),
                mode: UiMode::SelectingSlot {
                    card_id: "c1".into()
                },
            }
        );
    }

    #[test]
    fn a_playable_object_asks_for_a_holder() {
        assert_eq!(
            hand_click_command(true, CardType::Object, "o1", "MG-009"),
            UiCommand::SelectHandCard {
                instance_id: "o1".into(),
                mode: UiMode::SelectingEquipTarget {
                    object_id: "o1".into()
                },
            }
        );
    }

    #[test]
    fn events_and_ships_open_their_confirmation() {
        assert_eq!(
            hand_click_command(true, CardType::Event, "e1", "MG-023"),
            UiCommand::SelectHandCard {
                instance_id: "e1".into(),
                mode: UiMode::ConfirmEvent {
                    instance_id: "e1".into()
                },
            }
        );
        assert_eq!(
            hand_click_command(true, CardType::Ship, "s1", "MG-020"),
            UiCommand::SelectHandCard {
                instance_id: "s1".into(),
                mode: UiMode::ConfirmShip {
                    instance_id: "s1".into()
                },
            }
        );
    }

    #[test]
    fn a_counter_is_never_played_from_the_hand_loop() {
        assert_eq!(
            hand_click_command(true, CardType::Counter, "k1", "MG-026"),
            UiCommand::Ignore
        );
    }

    #[test]
    fn a_non_playable_card_opens_its_detail() {
        assert_eq!(
            hand_click_command(false, CardType::Character, "c1", "MG-001"),
            UiCommand::SetMode(UiMode::CardDetail {
                def_id: "MG-001".into(),
                instance_id: Some("c1".into()),
            })
        );
        // …whatever its type is.
        assert_eq!(
            hand_click_command(false, CardType::Event, "e1", "MG-023"),
            UiCommand::SetMode(UiMode::CardDetail {
                def_id: "MG-023".into(),
                instance_id: Some("e1".into()),
            })
        );
    }

    // --- drag routing ----------------------------------------

    #[test]
    fn only_playable_characters_and_objects_are_drag_sources() {
        assert_eq!(
            hand_drag_command(true, CardType::Character, "c1"),
            UiCommand::SelectHandCard {
                instance_id: "c1".into(),
                mode: UiMode::SelectingSlot {
                    card_id: "c1".into()
                },
            }
        );
        assert_eq!(
            hand_drag_command(true, CardType::Object, "o1"),
            UiCommand::SelectHandCard {
                instance_id: "o1".into(),
                mode: UiMode::SelectingEquipTarget {
                    object_id: "o1".into()
                },
            }
        );
        assert_eq!(hand_drag_command(true, CardType::Event, "e1"), UiCommand::Ignore);
        assert_eq!(hand_drag_command(true, CardType::Ship, "s1"), UiCommand::Ignore);
        assert_eq!(
            hand_drag_command(false, CardType::Character, "c1"),
            UiCommand::Ignore
        );
    }

    // --- footer hints ----------------------------------------

    #[test]
    fn hints_mirror_the_three_typescript_flags() {
        let none = footer_hints(&[GameAction::EndTurn]);
        assert_eq!(none, FooterHints::default());
        assert!(!none.any());

        let all = footer_hints(&[
            GameAction::FlipCaptain { slot: Slot::V1 },
            GameAction::UseHaki {
                haki_type: HakiType::King,
                target_instance_id: None,
            },
            GameAction::ActivateShip {
                ship_instance_id: "s1".into(),
            },
        ]);
        assert!(all.can_flip && all.can_king_haki && all.can_activate_ship);
        assert!(all.any());

        // Observation Haki is not the King's Haki hint.
        let observation = footer_hints(&[GameAction::UseHaki {
            haki_type: HakiType::Observation,
            target_instance_id: None,
        }]);
        assert!(!observation.can_king_haki);
    }

    // --- log --------------------------------------------------

    fn entry(turn: u32, player: PlayerId, message: &str) -> LogEntry {
        LogEntry {
            turn,
            player,
            message: message.into(),
        }
    }

    #[test]
    fn the_log_shows_the_last_twelve_newest_first() {
        let log: Vec<LogEntry> = (0..20)
            .map(|i| entry(i, PlayerId::Player1, &format!("line {i}")))
            .collect();
        let lines = log_lines(&log, PlayerId::Player1);
        assert_eq!(lines.len(), LOG_VISIBLE);
        assert_eq!(lines[0].message, "line 19");
        assert!(lines[0].newest);
        assert_eq!(lines[11].message, "line 8");
        assert!(!lines[11].newest);
    }

    #[test]
    fn the_log_marks_which_side_acted() {
        let log = vec![
            entry(1, PlayerId::Player1, "vous"),
            entry(1, PlayerId::Player2, "eux"),
        ];
        let lines = log_lines(&log, PlayerId::Player1);
        assert!(!lines[0].is_you, "newest first: the AI line comes first");
        assert!(lines[1].is_you);
    }

    #[test]
    fn a_short_log_is_returned_whole() {
        assert!(log_lines(&[], PlayerId::Player1).is_empty());
        let log = vec![entry(1, PlayerId::Player1, "a")];
        assert_eq!(log_lines(&log, PlayerId::Player1).len(), 1);
    }

    // --- head-less integration -------------------------------

    fn headless_app(session: Session) -> App {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins((BridgePlugin, SelectionPlugin, HandPlugin))
            .insert_resource(session);
        app
    }

    #[test]
    fn clicking_a_playable_character_arms_the_deploy_mode() {
        let mut session = make_session(7);
        // Turn 1 only affords `EndTurn`; play on until a deploy is legal.
        assert!(advance(&mut session, 80, |s| s
            .valid
            .iter()
            .any(|a| matches!(a, GameAction::DeployCharacter { .. }))));
        let instance_id = session
            .valid
            .iter()
            .find_map(|a| match a {
                GameAction::DeployCharacter { instance_id, .. } => Some(instance_id.clone()),
                _ => None,
            })
            .expect("a deployable card");

        let mut app = headless_app(session);
        app.world_mut()
            .write_message(HandCardClicked(instance_id.clone()));
        app.update();

        assert_eq!(
            *app.world().resource::<UiMode>(),
            UiMode::SelectingSlot {
                card_id: instance_id.clone()
            }
        );
        assert_eq!(
            app.world().resource::<SelectedHandCard>().0.as_deref(),
            Some(instance_id.as_str())
        );
    }

    #[test]
    fn clicking_an_unplayable_card_opens_its_detail_instead() {
        let session = make_session(3);
        // Turn 1: only `EndTurn` is legal, so nothing in hand is playable.
        let instance_id = session.you().hand[0].clone();
        let def_id = session.state.card(&instance_id).unwrap().def_id.clone();
        assert!(!hand_card_playable(&session.valid, &instance_id));

        let mut app = headless_app(session);
        app.world_mut()
            .write_message(HandCardClicked(instance_id.clone()));
        app.update();

        assert_eq!(
            *app.world().resource::<UiMode>(),
            UiMode::CardDetail {
                def_id,
                instance_id: Some(instance_id)
            }
        );
        assert!(app.world().resource::<SelectedHandCard>().0.is_none());
    }

    #[test]
    fn an_unknown_instance_id_is_swallowed() {
        let mut app = headless_app(make_session(11));
        app.world_mut()
            .write_message(HandCardClicked("does-not-exist".into()));
        app.update();
        assert_eq!(*app.world().resource::<UiMode>(), UiMode::Idle);
    }

    #[test]
    fn dragging_a_playable_object_arms_the_equip_mode() {
        let mut session = make_session(5);
        let found = advance(&mut session, 120, |s| {
            s.valid
                .iter()
                .any(|a| matches!(a, GameAction::EquipObject { .. }))
        });
        if !found {
            // Not every seed reaches an equip within the budget; the pure test
            // above already pins the routing.
            return;
        }
        let object_id = session
            .valid
            .iter()
            .find_map(|a| match a {
                GameAction::EquipObject {
                    object_instance_id, ..
                } => Some(object_instance_id.clone()),
                _ => None,
            })
            .expect("an equippable object");

        let mut app = headless_app(session);
        app.world_mut()
            .write_message(HandCardDragStarted(object_id.clone()));
        app.update();

        assert_eq!(
            *app.world().resource::<UiMode>(),
            UiMode::SelectingEquipTarget {
                object_id: object_id.clone()
            }
        );
        assert_eq!(
            app.world().resource::<SelectedHandCard>().0.as_deref(),
            Some(object_id.as_str())
        );
    }
}

//! The fanned hand and the footer that sits under it.
//!
//! Behaviour reference: the hand block, the "Actions" column and the log of
//! `src/components/Game.tsx`, plus `src/components/Card.tsx` /
//! `src/components/FullCard.tsx` for what a card shows. Visual reference: the
//! `footer` / `.hand` rules of `public/tests/board-hs-anime-v2.html`.
//!
//! Layout, top to bottom — mock-up `footer`, whose height is exactly
//! [`HAND_H`](crate::app::layout::HAND_H), the strip the board reserves for it:
//!
//! ```text
//!    ⚔ Clique ton Capitaine…      ← the hint box floats *above* the footer
//! ┌──────────────────────────────────────────────────────────┐
//! │ MAIN 5 ───────────────────────────────────────── DECK 41 │
//! │            ╱‾╲  ╱‾╲  ╱‾╲  ╱‾╲   ← the fan, centred       │
//! │            ╰──╯ ╰──╯ ╰──╯ ╰──╯                           │
//! │ [        Fin de tour ➡        ] [ ✕ ]      ← the one CTA │
//! │ T4 ► Zoro attaque Coby …                      (the log)  │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! The hint box is absolutely positioned above the footer on purpose: it comes
//! and goes with the legal actions, and the footer's height is the board's
//! reserve, which must never move.
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
use bevy::ui::widget::Text;
use bevy::window::PrimaryWindow;
use tcgop_engine::state::LogEntry;
use tcgop_engine::types::{CardType, GameAction, HakiType, PlayerId};

use crate::app::{
    AppScreen, AppSet, Fonts, Palette, board_active, board_ready, configure_pipeline, layout as L,
};
use crate::art::{ArtCache, Focus, faction_visual};
use crate::bridge::{BridgeSet, DispatchAction, Session};
use crate::selection::{
    SelectedHandCard, UiCommand, UiMode, hand_card_playable, on_hand_card_click,
};

use self::card_face::{
    ANCHOR, ARROW_RIGHT, ArtCover, CARET_LEFT, CARET_RIGHT, CROSS, CROSSED_SWORDS, CardFace,
    FaceCtx, STAR, spawn_card_face,
};
use self::layout::{
    HandCardState, NOMINAL_ART_ASPECT, cover_fit, fan_gap, fan_transform, preview_placement,
};

// ============================================================
// Plugin
// ============================================================

/// Fanned hand, hover preview, action column and rolling log.
pub struct HandPlugin;

impl Plugin for HandPlugin {
    fn build(&self, app: &mut App) {
        let can_load = app.world().contains_resource::<Assets<Font>>();
        let symbols = match app.world().get_resource::<AssetServer>() {
            Some(assets) if can_load => SymbolFont(assets.load(SYMBOL_FONT_PATH)),
            _ => SymbolFont::default(),
        };

        configure_pipeline(app);
        app.insert_resource(symbols)
            .init_resource::<HoveredHandCard>()
            .init_resource::<HandDrag>()
            .init_resource::<RefusalNotice>()
            .init_resource::<crate::vfx::ReducedMotion>()
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
                    .run_if(resource_exists::<Session>)
                    // Pointer observers fire in `PreUpdate`, one frame before
                    // `StateTransition` tears the board down: without this a
                    // click that lands as the winner is announced would be
                    // dispatched into a finished game.
                    .run_if(board_active),
            )
            // A refusal is read after the bridge applied (or refused) the
            // frame's actions, so the toast is up on the very same frame.
            .add_systems(
                Update,
                surface_engine_refusals
                    .in_set(AppSet::Refresh)
                    .after(BridgeSet)
                    .run_if(resource_exists::<Session>),
            )
            // Presentation: only when there is something to render into.
            //
            // `board_ready` — not just `resource_exists::<AssetServer>` — is
            // the condition every Board-entry spawner shares: `OnEnter` runs
            // once, so a screen entered without a `Session` would otherwise
            // come up as a footer floating over an empty terrain.
            .add_systems(OnEnter(AppScreen::Board), spawn_footer.run_if(board_ready))
            // …and again as soon as it can be, for the same reason the board
            // retries: `OnEnter` fires once and a screen entered without a
            // `Session` would stay footer-less for good.
            .add_systems(
                Update,
                spawn_footer
                    .in_set(AppSet::Render)
                    .run_if(in_state(AppScreen::Board))
                    .run_if(board_ready)
                    .run_if(|footers: Query<(), With<FooterRoot>>| footers.is_empty()),
            )
            // The hover id survives a game otherwise, and instance ids repeat
            // verbatim between games, so a stale hover would open a full-size
            // preview with no pointer over the hand.
            .add_systems(OnEnter(AppScreen::Board), forget_pointer_state)
            .add_systems(OnExit(AppScreen::Board), forget_pointer_state)
            .add_systems(
                Update,
                (
                    rebuild_hand,
                    (
                        fit_fan_width,
                        apply_fan_transforms,
                        sync_playable_pips,
                        update_notice,
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
/// The board **finishes** the gesture: every cell observes `Pointer<DragDrop>`
/// (`board::on_cell_dropped`) and routes it through
/// [`on_cell_drop`](crate::selection::on_cell_drop). By the time the pointer is
/// over a cell the [`UiMode`] is already `SelectingSlot` /
/// `SelectingEquipTarget`, so the drop dispatches exactly what a click on that
/// cell would have — releasing a dragged card over a legal slot deploys it,
/// with no extra click.
///
/// `source` is the wrapper entity the gesture started on. The DOM refuses to
/// *begin* a drag anywhere but a playable character / object
/// (`draggable={canPlay && dragType}`); Bevy has no such gate, so the entity
/// travels with the message and [`HandDrag`] remembers it — that is what lets
/// a drop check **which** card was released instead of firing whatever
/// selection happened to be armed.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct HandCardDragStarted {
    pub instance_id: String,
    pub source: Entity,
}

// ============================================================
// Resources
// ============================================================

/// TS `hoveredHand` — the instance id under the pointer, if any.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct HoveredHandCard(pub Option<String>);

/// The hand-to-board drag in flight — the Bevy stand-in for the DOM's
/// `draggable` attribute *and* its `dragend` event.
///
/// The web gets both for free: a non-playable card, a Ship / Event / Counter
/// and every board tile simply refuse to start a drag, and a drag released
/// anywhere useless fires `dragend`, which drops the gesture. Bevy makes every
/// observed node a drag source and only reports the drop, so both halves are
/// tracked here:
///
/// - `source` is the entity the accepted gesture started on, so
///   [`board::on_cell_dropped`](crate::board) can refuse a drop that carries
///   anything else;
/// - `consumed` records that a cell took the gesture, so the `DragEnd`
///   observer knows whether the release landed somewhere — without it a drag
///   let go over the footer, the waterline or outside the window would leave
///   [`UiMode`] latched in a selecting mode with no pointer escape.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct HandDrag {
    /// The card being carried, once the gesture was accepted.
    pub instance_id: Option<String>,
    /// The wrapper entity that card is drawn by.
    pub source: Option<Entity>,
    /// Pointer travel since the drag began, in logical pixels.
    pub offset: Vec2,
    /// A cell finished the gesture; `DragEnd` must not undo it.
    pub consumed: bool,
}

impl HandDrag {
    /// A gesture is in flight.
    pub fn is_dragging(&self) -> bool {
        self.instance_id.is_some()
    }

    /// `entity` is the card this drag is carrying — the check the DOM performs
    /// for free by never starting a drag anywhere else.
    pub fn carries(&self, entity: Entity) -> bool {
        self.instance_id.is_some() && self.source == Some(entity)
    }

    /// Arm the gesture on an accepted card.
    fn begin(&mut self, instance_id: String, source: Entity) {
        *self = HandDrag {
            instance_id: Some(instance_id),
            source: Some(source),
            offset: Vec2::ZERO,
            consumed: false,
        };
    }

    /// Forget the gesture, whatever came of it.
    fn end(&mut self) {
        *self = HandDrag::default();
    }
}

/// A refusal the player must be told about — see [`surface_engine_refusals`].
///
/// `bridge` documents [`EngineErrorEvent`](crate::bridge::EngineErrorEvent) as
/// a bug report, but the crate's own scripted bot knows better: `e2e.rs` keeps
/// a `refused` set because `valid_actions_for` is occasionally more permissive
/// than `apply` (MG-024 *Flashback* is offered with an empty graveyard and
/// then rejected). A human clicking that same card would otherwise get a
/// completely silent dead click — the card stays in hand, the log does not
/// move, nothing at all happens on screen.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct RefusalNotice {
    /// What to show, if anything.
    pub message: Option<String>,
    /// Seconds left on screen.
    pub remaining: f32,
}

impl RefusalNotice {
    /// How long a refusal stays up.
    pub const SECS: f32 = 3.0;

    /// The sentence shown for a refused action.
    pub fn wording(action: &GameAction) -> String {
        let what = match action {
            GameAction::PlayEvent { .. } => "Cet évènement",
            GameAction::PlayCounter { .. } => "Ce contre",
            GameAction::DeployCharacter { .. } => "Ce personnage",
            GameAction::DeployShip { .. } => "Ce navire",
            GameAction::EquipObject { .. } => "Cet équipement",
            GameAction::BaseAttack { .. }
            | GameAction::SpecialAttack { .. }
            | GameAction::CaptainAttack { .. } => "Cette attaque",
            _ => "Cette action",
        };
        format!("{what} n'est pas jouable pour l'instant.")
    }

    fn show(&mut self, message: String) {
        self.message = Some(message);
        self.remaining = Self::SECS;
    }
}

/// What the fan was last built from, so it is only rebuilt when it changed.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
struct HandView {
    /// The instance ids in hand, in order — **not** their playability, which
    /// flips twice a turn and is applied without a respawn.
    key: Vec<String>,
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
    /// What the green pip currently shows — see [`sync_playable_pips`].
    pub playable: bool,
}

/// The footer itself, so a missed `OnEnter` can be retried.
#[derive(Component)]
struct FooterRoot;

/// The row the fan lives in.
#[derive(Component)]
struct HandCardsRow;

/// The transient "the engine refused that" toast, and its label.
#[derive(Component)]
struct NoticeNode;

#[derive(Component)]
struct NoticeText;

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
    /// `⚡ Ton Capitaine a un pouvoir à dépenser` — the captain's surcharge
    /// (§8.34(b)) or an awakened fruit it wears (§8.28 follow-up × §8.48),
    /// both of which live behind the captain menu and would otherwise never
    /// be looked for.
    CaptainPower,
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
    /// §8.34(b) / §8.28 follow-up — the captain has a surcharge or an awakened
    /// fruit special it may spend right now.
    pub can_captain_power: bool,
}

impl FooterHints {
    pub fn any(&self) -> bool {
        self.can_flip || self.can_king_haki || self.can_activate_ship || self.can_captain_power
    }

    fn shows(&self, node: HintNode) -> bool {
        match node {
            HintNode::Box => self.any(),
            HintNode::Flip => self.can_flip,
            HintNode::KingHaki => self.can_king_haki,
            HintNode::Ship => self.can_activate_ship,
            HintNode::CaptainPower => self.can_captain_power,
        }
    }
}

/// Derive the hints from the human's legal actions.
pub fn footer_hints(valid: &[GameAction]) -> FooterHints {
    FooterHints {
        can_captain_power: valid.iter().any(|a| match a {
            GameAction::UseSurcharge { .. } => true,
            GameAction::FruitSpecialAttack {
                attacker_instance_id,
                ..
            } => crate::selection::is_captain_key(attacker_instance_id),
            _ => false,
        }),
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
        UiCommand::DispatchKeepUi(action) => {
            dispatch.write(DispatchAction(action));
        }
        UiCommand::SetMode(next) => *mode = next,
        UiCommand::SelectHandCard {
            instance_id,
            mode: next,
        } => {
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

/// A drag started on a hand card.
///
/// The gesture is only **armed** when [`hand_drag_command`] accepts it — the
/// DOM's `draggable={canPlay && dragType}`. A refused card (non-playable, or a
/// Ship / Event / Counter) leaves both the mode and [`HandDrag`] exactly as
/// they were, so releasing it over a lit cell cannot complete somebody else's
/// selection.
fn handle_hand_drags(
    mut drags: MessageReader<HandCardDragStarted>,
    session: Res<Session>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut drag: ResMut<HandDrag>,
    mut dispatch: MessageWriter<DispatchAction>,
) {
    for started in drags.read() {
        let instance_id = &started.instance_id;
        let Some(card) = session.state.card(instance_id) else {
            continue;
        };
        let Some(def) = session.registry.card_def(&card.def_id) else {
            continue;
        };
        let playable = hand_card_playable(&session.valid, instance_id);
        let command = hand_drag_command(playable, def.card_type, instance_id);
        if command == UiCommand::Ignore {
            continue;
        }
        apply_ui_command(command, &mut mode, &mut selected, &mut dispatch);
        drag.begin(instance_id.clone(), started.source);
    }
}

/// Surface an engine refusal to the player — gap-filler for the "silent dead
/// click" of a `valid` action `apply` then rejects.
fn surface_engine_refusals(
    mut failures: MessageReader<crate::bridge::EngineErrorEvent>,
    session: Res<Session>,
    mut notice: ResMut<RefusalNotice>,
) {
    for failure in failures.read() {
        // The AI driver has its own recovery path (it re-picks another action);
        // only the human needs to be told why nothing happened.
        if session.is_ai_turn() {
            continue;
        }
        notice.show(RefusalNotice::wording(&failure.action));
    }
}

/// Tick the refusal toast down and mirror it into the footer.
fn update_notice(
    time: Res<Time>,
    mut notice: ResMut<RefusalNotice>,
    mut boxes: Query<&mut Node, With<NoticeNode>>,
    mut labels: Query<&mut Text, With<NoticeText>>,
) {
    if notice.remaining > 0.0 {
        notice.remaining = (notice.remaining - time.delta_secs()).max(0.0);
        if notice.remaining == 0.0 {
            notice.message = None;
        }
    }
    let shown = notice.message.clone();
    if let Ok(mut node) = boxes.single_mut() {
        let display = if shown.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
    if let (Ok(mut text), Some(message)) = (labels.single_mut(), shown)
        && text.0 != message
    {
        text.0 = message;
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

fn symbol_font(symbols: &SymbolFont, size: f32) -> TextFont {
    TextFont {
        font: symbols.0.clone().into(),
        font_size: FontSize::Px(size),
        ..default()
    }
}

/// Reset the pointer caches when the board is entered **or** left.
///
/// `spawn_footer` already forgets the two "already drawn" caches; this one is
/// separate because it must also run on the way out — instance ids are
/// `{def_id}_{n}_0` for a seeded context, i.e. identical from one game to the
/// next, so a hover (or a drag whose card was despawned mid-gesture) left over
/// from game 1 matches a real card in game 2.
fn forget_pointer_state(
    mut hovered: ResMut<HoveredHandCard>,
    mut drag: ResMut<HandDrag>,
    mut notice: ResMut<RefusalNotice>,
) {
    *hovered = HoveredHandCard::default();
    *drag = HandDrag::default();
    *notice = RefusalNotice::default();
}

/// The footer: the fan and the single CTA row, plus the boxes that float
/// above it (hints, the refusal toast, the log).
///
/// Its height is exactly [`L::HAND_H`], which is what the board reserves for
/// it (`board::spawn_board`'s `HandReserve`), so YOUR command bar always stays
/// visible above the cards instead of being covered — and clickable, since the
/// footer blocks the picking below it.
///
/// Mock-up `footer`: `.hand` centred across the whole width, then one `.cta`
/// row (`Fin de tour ➜` + `✕`). There is no action column: the hint box floats
/// **above** the footer so its appearing and disappearing can never change the
/// reserved height.
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
            FooterRoot,
            Name::new("HandFooter"),
            Node {
                position_type: PositionType::Absolute,
                left: px(0.),
                right: px(0.),
                bottom: px(0.),
                height: px(L::HAND_H),
                flex_direction: FlexDirection::Column,
                row_gap: px(L::FOOTER_ROW_GAP),
                padding: UiRect {
                    left: px(L::HAND_PAD_X),
                    right: px(L::HAND_PAD_X),
                    top: px(L::FOOTER_PAD_TOP),
                    bottom: px(L::FOOTER_PAD_BOTTOM),
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
            spawn_hint_box(footer, &pal, &fonts, &symbols);
            spawn_notice(footer, &pal, &fonts);
            spawn_log_panel(footer, &pal);
            spawn_hand_column(footer);
            spawn_cta_row(footer, &pal, &fonts, &symbols);
        });
}

/// The fan row.
///
/// The mock-up's footer is `.hand` + `.cta` and nothing else: the "MAIN n
/// ──── DECK n" strip that used to sit here is gone, because the same two
/// counts are already drawn once per side inside the command bar
/// (`board::widgets::counts_row`, mock-up `.res > .counts`) and every pixel
/// the footer reserves is a pixel the two halves lose.
fn spawn_hand_column(footer: &mut ChildSpawnerCommands) {
    // The fan itself — mock-up `.hand{justify-content:center}`, spanning the
    // whole footer so its centre is the window's centre.
    footer.spawn((
        HandCardsRow,
        Node {
            width: percent(100.),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::End,
            column_gap: px(L::HAND_GAP),
            // The outer cards drop by `HAND_FAN_LIFT_MAX`, so the row reserves
            // that much under them.
            height: px(L::HAND_CARD_H + L::HAND_FAN_LIFT_MAX),
            flex_shrink: 0.,
            padding: UiRect::bottom(px(L::HAND_FAN_LIFT_MAX)),
            ..default()
        },
    ));
}

/// Mock-up `.cta` — the client's **only** "Fin de tour" / "Annuler" pair.
fn spawn_cta_row(
    footer: &mut ChildSpawnerCommands,
    pal: &Palette,
    fonts: &Fonts,
    symbols: &SymbolFont,
) {
    footer
        .spawn(Node {
            height: px(L::BUTTON_H),
            flex_shrink: 0.,
            flex_direction: FlexDirection::Row,
            column_gap: px(9.),
            ..default()
        })
        .with_children(|cta| {
            // `.btn` — the gold→amber gradient, dark label.
            cta.spawn((
                EndTurnButton { disabled: false },
                Button,
                Node {
                    flex_grow: 1.,
                    height: percent(100.),
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
                        TextFont {
                            font: fonts.poppins_bold.clone().into(),
                            font_size: FontSize::Px(14.),
                            ..default()
                        },
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

            // `.btn-x` — a bare square, shown only while the UI is not idle.
            cta.spawn((
                CancelButton,
                Button,
                Node {
                    display: Display::None,
                    width: px(52.),
                    height: percent(100.),
                    flex_shrink: 0.,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(1.)),
                    border_radius: BorderRadius::all(px(L::BUTTON_RADIUS)),
                    ..default()
                },
                BorderColor::all(pal.ink),
                children![(
                    Text::new(CROSS),
                    symbol_font(symbols, 14.),
                    TextColor(pal.text),
                    Pickable::IGNORE,
                ),],
            ))
            .observe(cancel_clicked);
        });
}

/// The amber "clique ton Capitaine / ton Navire" hints.
///
/// Absolutely positioned **above** the footer: it appears and disappears with
/// the legal actions, and the footer's height is the board's reserve, which
/// must never move.
fn spawn_hint_box(
    footer: &mut ChildSpawnerCommands,
    pal: &Palette,
    fonts: &Fonts,
    symbols: &SymbolFont,
) {
    footer
        .spawn((
            HintNode::Box,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: px(L::HAND_PAD_X),
                bottom: percent(100.),
                flex_direction: FlexDirection::Column,
                row_gap: px(2.),
                padding: UiRect::axes(px(8.), px(6.)),
                border: UiRect::all(px(1.)),
                border_radius: BorderRadius::all(px(10.)),
                ..default()
            },
            BackgroundColor(pal.gold.with_alpha(0.12)),
            BorderColor::all(pal.gold.with_alpha(0.30)),
            Pickable::IGNORE,
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
                (
                    HintNode::CaptainPower,
                    STAR,
                    "Pouvoir de Capitaine dispo (clique-le)",
                ),
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
                        Pickable::IGNORE,
                    ))
                    .with_children(|entry| {
                        entry.spawn((
                            Text::new(glyph),
                            symbol_font(symbols, 9.),
                            TextColor(pal.amber),
                        ));
                        entry.spawn((Text::new(text), label_font(fonts, 9.), TextColor(pal.amber)));
                    });
            }
        });
}

/// The rolling log.
///
/// Absolutely positioned **above** the footer, like the hint box and for the
/// same reason: the mock-up's footer contains only `.hand` and `.cta`, and the
/// footer's height is the reserve the terrain gives up
/// ([`L::HAND_H`]). Anchored right so it never collides with the hints.
fn spawn_log_panel(footer: &mut ChildSpawnerCommands, pal: &Palette) {
    footer.spawn((
        LogPanel,
        Node {
            position_type: PositionType::Absolute,
            right: px(L::HAND_PAD_X),
            bottom: percent(100.),
            width: px(340.),
            max_width: percent(50.),
            height: px(L::LOG_H),
            flex_shrink: 0.,
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

/// The transient "the engine refused that" toast — see [`RefusalNotice`].
fn spawn_notice(footer: &mut ChildSpawnerCommands, pal: &Palette, fonts: &Fonts) {
    footer.spawn((
        NoticeNode,
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            left: percent(50.),
            bottom: percent(100.),
            max_width: percent(70.),
            padding: UiRect::axes(px(12.), px(7.)),
            border: UiRect::all(px(1.)),
            border_radius: BorderRadius::all(px(10.)),
            ..default()
        },
        UiTransform::from_translation(Val2::percent(-50., -18.)),
        BackgroundColor(pal.bg_deep.with_alpha(0.92)),
        BorderColor::all(pal.target.with_alpha(0.55)),
        Pickable::IGNORE,
        children![(
            NoticeText,
            Text::new(String::new()),
            TextFont {
                font: fonts.poppins_semi.clone().into(),
                font_size: FontSize::Px(L::FS_LABEL + 1.0),
                ..default()
            },
            TextColor(pal.text),
            Pickable::IGNORE,
        )],
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
        drags.write(HandCardDragStarted {
            instance_id: card.instance_id.clone(),
            source: event.entity,
        });
    }
}

/// The dragged card follows the pointer, so the gesture is visible.
///
/// Without it the card stays parked in the fan and nothing on screen says a
/// drag is in flight — `hand_card_drag_started` even clears the hover preview.
fn hand_card_dragged(event: On<Pointer<Drag>>, mut drag: ResMut<HandDrag>) {
    if drag.carries(event.entity) {
        drag.offset = event.distance;
    }
}

/// The gesture was released — TS `dragend`.
///
/// A drop over a cell already ran (`DragDrop` is triggered before `DragEnd`)
/// and set `consumed`. Anything else — the footer, the header, the terrain
/// background, the waterline, outside the window — lands here with the
/// selection still armed, and *this* is what returns the UI to idle instead of
/// leaving it latched with every non-eligible cell dimmed and no pointer
/// escape but the `✕ Annuler` button.
fn hand_card_drag_ended(
    event: On<Pointer<DragEnd>>,
    mut drag: ResMut<HandDrag>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
) {
    if !drag.carries(event.entity) {
        return;
    }
    if !drag.consumed {
        *mode = UiMode::Idle;
        selected.0 = None;
    }
    drag.end();
}

// ============================================================
// Rendering systems
// ============================================================

/// Rebuild the fan whenever the **contents** of the hand changed.
///
/// Playability deliberately stays out of the rebuild key: `Session::valid` is
/// emptied for the whole AI turn, so keying on it would tear the fan down and
/// respawn it at least twice per turn cycle — destroying the drag gesture in
/// flight, the picking hover entity and the pip's pulse each time. The flag
/// lives on [`HandCard`] instead and [`sync_playable_pips`] toggles the pip.
#[allow(clippy::too_many_arguments)]
fn rebuild_hand(
    mut commands: Commands,
    session: Res<Session>,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    assets: Res<AssetServer>,
    mut art: ResMut<ArtCache>,
    mut view: ResMut<HandView>,
    mut hovered: ResMut<HoveredHandCard>,
    mut drag: ResMut<HandDrag>,
    row: Query<Entity, With<HandCardsRow>>,
) {
    let Ok(row) = row.single() else {
        return;
    };
    let hand = &session.you().hand;
    let key: Vec<String> = hand.to_vec();
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

    // Same for a gesture whose card left the hand while it was in flight: the
    // wrapper is despawned here, so its `DragEnd` will never arrive and the
    // drag would stay armed — and armed means `on_cell_dropped` would accept a
    // dangling entity id and `UiMode` would never be released.
    if drag.is_dragging()
        && drag
            .instance_id
            .as_ref()
            .is_some_and(|id| !hand.iter().any(|card| card == id))
    {
        *drag = HandDrag::default();
    }

    commands.entity(row).despawn_related::<Children>();

    let pal = *palette;
    let tiles: Vec<(usize, String, Option<Handle<Image>>, Color)> = view
        .key
        .iter()
        .enumerate()
        .filter_map(|(index, instance_id)| {
            let card = session.state.card(instance_id)?;
            let handle = art.card(&assets, &card.def_id, card.is_awakened.unwrap_or(false));
            let wash = session
                .registry
                .card_def(&card.def_id)
                .map(|def| faction_visual(def.faction).ambiance)
                .unwrap_or(pal.bg_slot);
            Some((index, instance_id.clone(), handle, wash))
        })
        .collect();

    commands.entity(row).with_children(|row| {
        if tiles.is_empty() {
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

        for (index, instance_id, handle, wash) in tiles {
            spawn_hand_tile(row, &pal, index, instance_id, handle, wash);
        }
    });
}

/// One card of the fan — mock-up `.h`: a cropped illustration in a rounded
/// frame, and nothing else.
///
/// The full frame (cost, PV, DEF, traits, rules text) lives one pointer-move
/// away, in the hover preview, exactly as the mock-up intends
/// ("pleine illustration" on the board, details on demand).
fn spawn_hand_tile(
    row: &mut ChildSpawnerCommands,
    pal: &Palette,
    index: usize,
    instance_id: String,
    handle: Option<Handle<Image>>,
    wash: Color,
) {
    let (w, h) = (L::HAND_CARD_W, L::HAND_CARD_H);
    let focus = Focus::new(0.5, L::HAND_ART_FOCUS_Y);

    let mut wrapper = row.spawn((
        Node {
            width: px(w),
            height: px(h),
            flex_shrink: 0.,
            ..default()
        },
        UiTransform::IDENTITY,
    ));
    let mut face_entity = Entity::PLACEHOLDER;
    wrapper.with_children(|w_| {
        let mut face = w_.spawn((
            Node {
                width: percent(100.),
                height: percent(100.),
                overflow: Overflow::clip(),
                border: UiRect::all(px(1.)),
                border_radius: BorderRadius::all(px(13.)),
                ..default()
            },
            BackgroundColor(wash),
            BorderColor::all(pal.ink),
            Pickable::IGNORE,
        ));
        face.with_children(|face| {
            if let Some(image) = handle {
                let fit = cover_fit(w, h, NOMINAL_ART_ASPECT, focus);
                face.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(fit.left),
                        top: px(fit.top),
                        width: px(fit.width),
                        height: px(fit.height),
                        ..default()
                    },
                    ImageNode::new(image).with_mode(NodeImageMode::Stretch),
                    ArtCover {
                        box_w: w,
                        box_h: h,
                        focus,
                        applied_aspect: NOMINAL_ART_ASPECT,
                    },
                    Pickable::IGNORE,
                ));
            }
        });
        face_entity = face.id();

        w_.spawn((
            PlayablePip,
            Node {
                display: Display::None,
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
    });
    wrapper.insert(HandCard {
        instance_id,
        index,
        face: face_entity,
        playable: false,
    });
    wrapper
        .observe(hand_card_entered)
        .observe(hand_card_left)
        .observe(hand_card_clicked)
        .observe(hand_card_drag_started)
        .observe(hand_card_dragged)
        .observe(hand_card_drag_ended);
}

/// Show the green pip under every card the engine currently allows.
///
/// This is the half of the old rebuild key that survived: the flag changes at
/// least twice per turn cycle, and it must not cost a respawn.
fn sync_playable_pips(
    session: Res<Session>,
    mut cards: Query<(&mut HandCard, &Children)>,
    mut pips: Query<&mut Node, With<PlayablePip>>,
) {
    for (mut card, children) in &mut cards {
        let playable = hand_card_playable(&session.valid, &card.instance_id);
        if card.playable == playable {
            continue;
        }
        card.playable = playable;
        for child in children.iter() {
            if let Ok(mut node) = pips.get_mut(child) {
                node.display = if playable {
                    Display::Flex
                } else {
                    Display::None
                };
            }
        }
    }
}

/// The fan (rotation + lift) and the gold frame of the selected card.
fn apply_fan_transforms(
    hovered: Res<HoveredHandCard>,
    selected: Res<SelectedHandCard>,
    drag: Res<HandDrag>,
    palette: Res<Palette>,
    cards: Query<(&HandCard, &mut UiTransform)>,
    mut borders: Query<&mut BorderColor>,
) {
    let count = cards.iter().count();
    for (card, mut transform) in cards {
        let dragged = drag.instance_id.as_deref() == Some(card.instance_id.as_str());
        let state = HandCardState {
            hovered: hovered.0.as_deref() == Some(card.instance_id.as_str()),
            selected: selected.0.as_deref() == Some(card.instance_id.as_str()),
        };
        let fan = fan_transform(card.index, count, state, L::HAND_CARD_H);
        // The card in flight leaves the fan and follows the pointer — the
        // gesture has to be visible, the hover preview is closed by then.
        let (drag_x, drag_y) = if dragged {
            (drag.offset.x, drag.offset.y)
        } else {
            (0.0, 0.0)
        };
        *transform = UiTransform {
            translation: Val2::px(fan.offset_x + drag_x, fan.offset_y + drag_y),
            scale: Vec2::splat(if dragged { fan.scale * 1.06 } else { fan.scale }),
            rotation: Rot2::degrees(if dragged { 0.0 } else { fan.rotation_deg }),
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
///
/// The card is only respawned when the hovered *id* changes; its placement is
/// recomputed **every frame**, so the preview follows the fan when the window
/// is resized or the hand re-lays itself out instead of staying frozen where
/// the hover began.
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
    mut existing: Query<(Entity, &HandPreview, &mut Node)>,
) {
    let wanted = hovered
        .0
        .as_ref()
        .filter(|_| mode.is_idle() && !session.in_counter_window());

    let Some(instance_id) = wanted else {
        for (entity, _, _) in &existing {
            commands.entity(entity).despawn();
        }
        return;
    };

    // Where the preview belongs, from the card's *current* box.
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

    // Already showing this card: re-place it and stop.
    if let Ok((_, shown, mut node)) = existing.single_mut()
        && &shown.0 == instance_id
    {
        node.left = px(placement.left);
        node.top = px(placement.top);
        return;
    }
    for (entity, _, _) in &existing {
        commands.entity(entity).despawn();
    }

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
///
/// Reduced motion parks it at full strength instead: this and the board's
/// attack rings are the only never-ending loops on screen, i.e. exactly what a
/// `prefers-reduced-motion` guard exists for.
fn pulse_playable_pips(
    time: Res<Time>,
    reduced: Res<crate::vfx::ReducedMotion>,
    palette: Res<Palette>,
    pips: Query<&mut BackgroundColor, With<PlayablePip>>,
) {
    let alpha = if reduced.enabled {
        1.0
    } else {
        let t = time.elapsed_secs() * 3.0;
        0.55 + 0.45 * (t.sin() * 0.5 + 0.5)
    };
    let color = palette.deploy.with_alpha(alpha);
    for mut background in pips {
        // Writing an unchanged colour every frame would keep re-triggering
        // change detection, which is the very thing reduced motion is meant
        // to stop.
        if background.0 != color {
            background.0 = color;
        }
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
        assert_eq!(
            hand_drag_command(true, CardType::Event, "e1"),
            UiCommand::Ignore
        );
        assert_eq!(
            hand_drag_command(true, CardType::Ship, "s1"),
            UiCommand::Ignore
        );
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

    /// The fan must survive a playability flip.
    ///
    /// `Session::valid` is emptied for the whole AI turn, so every card's flag
    /// goes false on `EndTurn` and true again when the turn comes back. Keying
    /// the rebuild on it tore the fan down twice per turn cycle, taking the
    /// drag in flight, the picking hover entity and the pip's pulse with it.
    #[test]
    fn flipping_playability_never_respawns_the_fan() {
        use crate::app::{AppScreen, Fonts, Palette};
        use crate::art::ArtCache;

        let mut session = make_session(7);
        assert!(advance(&mut session, 80, |s| s
            .valid
            .iter()
            .any(|a| matches!(a, GameAction::DeployCharacter { .. }))));

        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(bevy::state::app::StatesPlugin)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .init_asset::<Image>()
            .init_state::<AppScreen>()
            .insert_resource(Palette::default())
            .insert_resource(Fonts::default())
            .init_resource::<ArtCache>()
            .add_plugins((BridgePlugin, SelectionPlugin, HandPlugin))
            .insert_resource(session);
        app.world_mut()
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::Board);
        app.update();
        app.update();

        let cards = |app: &mut App| -> Vec<(Entity, String, bool)> {
            let mut q = app.world_mut().query::<(Entity, &HandCard)>();
            let mut out: Vec<(Entity, String, bool)> = q
                .iter(app.world())
                .map(|(e, c)| (e, c.instance_id.clone(), c.playable))
                .collect();
            out.sort_by(|a, b| a.1.cmp(&b.1));
            out
        };
        let before = cards(&mut app);
        assert!(!before.is_empty(), "the fan is on screen");
        assert!(
            before.iter().any(|(_, _, playable)| *playable),
            "something is playable on your own turn"
        );

        // Hand the turn over: every card becomes unplayable.
        app.world_mut()
            .write_message(DispatchAction(GameAction::EndTurn));
        app.update();

        let after = cards(&mut app);
        assert_eq!(
            before.iter().map(|c| c.0).collect::<Vec<_>>(),
            after.iter().map(|c| c.0).collect::<Vec<_>>(),
            "the card entities must be the very same ones"
        );
        assert!(
            after.iter().all(|(_, _, playable)| !*playable),
            "…but their pips went out"
        );
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
        let source = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(HandCardDragStarted {
            instance_id: object_id.clone(),
            source,
        });
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

        // …and the gesture is armed on that entity, so only *its* drop can
        // finish it.
        let drag = app.world().resource::<HandDrag>();
        assert!(drag.is_dragging());
        assert!(drag.carries(source));
        assert!(!drag.consumed);
    }

    /// A drag the web would refuse to start (`draggable={canPlay && dragType}`)
    /// must leave the state machine exactly as it found it — including a
    /// selection somebody else armed, which a later drop would otherwise
    /// complete on the wrong card.
    #[test]
    fn a_refused_drag_arms_nothing_and_disturbs_nothing() {
        let session = make_session(7);
        let mut app = headless_app(session);

        // Pretend a slot selection is already running.
        let armed = UiMode::SelectingSlot {
            card_id: "somebody-else".into(),
        };
        *app.world_mut().resource_mut::<UiMode>() = armed.clone();
        app.world_mut().resource_mut::<SelectedHandCard>().0 = Some("somebody-else".into());

        // An id that is not in hand at all is the simplest "not draggable".
        let source = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(HandCardDragStarted {
            instance_id: "not-a-card".into(),
            source,
        });
        app.update();

        assert_eq!(*app.world().resource::<UiMode>(), armed);
        assert!(!app.world().resource::<HandDrag>().is_dragging());
        assert!(!app.world().resource::<HandDrag>().carries(source));
    }

    /// An engine refusal on your own turn raises the footer toast instead of
    /// being a silent dead click.
    #[test]
    fn a_refused_action_tells_the_player() {
        let session = make_session(9);
        let mut app = headless_app(session);
        app.world_mut()
            .write_message(DispatchAction(GameAction::PlayEvent {
                instance_id: "not-in-hand".into(),
                targets: None,
            }));
        app.update();

        let notice = app.world().resource::<RefusalNotice>();
        assert!(
            notice.message.is_some(),
            "the player must be told the click did nothing"
        );
        assert!(notice.remaining > 0.0);
    }

    #[test]
    fn a_refusal_names_what_was_refused() {
        let wording = RefusalNotice::wording(&GameAction::PlayEvent {
            instance_id: "x".into(),
            targets: None,
        });
        assert!(wording.contains("\u{00E8}nement"), "{wording}");
        assert!(RefusalNotice::wording(&GameAction::EndTurn).contains("action"));
    }
}

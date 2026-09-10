//! The battlefield: one flat terrain, the opponent's ship deck on top, yours
//! below, cut by a luminous waterline.
//!
//! Visual target `public/tests/board-hs-anime-v2.html`, behaviour reference
//! `renderHalf` / `renderLine` / `renderCommand` of `src/components/Game.tsx`.
//!
//! **Shape of the module.** Everything that decides *what* to draw is pure and
//! head-lessly tested:
//! - [`model::board_view`] turns `(Session, UiMode)` into a comparable
//!   [`BoardView`](model::BoardView) — tiles, rings, dimming, command bars,
//!   header;
//! - [`geometry`] fits a half into the space the header and the hand leave and
//!   does the `background-size: cover` crop of the illustrations;
//! - [`interaction::apply_ui_command`] is the TS `dispatch(...) + resetUI()`
//!   rule.
//!
//! The systems below only spawn and diff. The **skeleton** (halves, floors,
//! rows, cells, command bars, header) is spawned once on
//! `OnEnter(AppScreen::Board)`; every frame the view is rebuilt and compared
//! per widget, so a tile whose content did not change is never respawned and
//! only its ring / dim veil is re-tinted.
//!
//! Clicks are routed through the pure helpers of [`crate::selection`]
//! ([`on_cell_click`](crate::selection::on_cell_click),
//! [`on_captain_click`](crate::selection::on_captain_click)); a
//! [`UiCommand::Dispatch`](crate::selection::UiCommand) writes
//! [`DispatchAction`](crate::bridge::DispatchAction) **and** resets the UI.
//! Cells also observe `Pointer<DragDrop>`, so a card dragged out of the hand is
//! played by releasing it over a slot
//! ([`on_cell_drop`](crate::selection::on_cell_drop)).
//!
//! The board owns **no button**: the mock-up has a single `.cta` row, and
//! [`crate::hand`]'s footer builds it. The header is brand / turn ring /
//! status pill and nothing else — the foe's counts and ship are drawn once, in
//! the foe command bar.
//!
//! **Responsive.** [`BoardMetrics`] resolves every box from the window size and
//! `relayout_on_resize` re-derives it on `WindowResized`, so the tiles keep the
//! mock-up's aspect and the battlefield column follows the window instead of
//! being pinned to a design-time constant.

pub mod geometry;
pub mod interaction;
pub mod model;
pub mod style;
pub mod widgets;

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowResized};
use tcgop_engine::types::{PlayerId, Slot};

use crate::app::{
    AppScreen, AppSet, Fonts, Palette, board_ready, configure_pipeline, layout as l,
};
use crate::hand::SymbolFont;
use crate::art::{ArtCache, Focus};
use crate::bridge::{BridgeSet, DispatchAction, Session};
use crate::selection::{
    SelectedHandCard, UiCommand, UiMode, on_captain_click, on_cell_click, on_cell_drop,
};

use geometry::{HalfMetrics, cover, half_available_h, half_metrics};
use interaction::apply_ui_command;
use model::{
    BoardView, CaptainCardView, CellContent, CellView, HeaderView, ResourceView, Ring, ShipView,
    board_view,
};
use widgets::{CoverFit, Painter, fill_node};

// ============================================================
// Plugin
// ============================================================

/// Terrain, command bars, slots and unit tiles.
pub struct BoardPlugin;

/// Every board system, so other features can order against them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoardSet;

impl Plugin for BoardPlugin {
    fn build(&self, app: &mut App) {
        configure_pipeline(app);
        // The board's stat chips are glyphs (⚔ / 🛡): they need the one font in
        // `assets/fonts` with symbol coverage. `HandPlugin` normally owns it;
        // this keeps `BoardPlugin` usable on its own, whatever the plugin order.
        if !app.world().contains_resource::<SymbolFont>() {
            // `Assets<Font>` only exists once the text plugins are in: a
            // head-less app with a bare `AssetPlugin` would panic on `load`.
            let can_load = app.world().contains_resource::<Assets<Font>>();
            let symbols = match app.world().get_resource::<AssetServer>() {
                Some(assets) if can_load => {
                    SymbolFont(assets.load(crate::hand::SYMBOL_FONT_PATH))
                }
                _ => SymbolFont::default(),
            };
            app.insert_resource(symbols);
        }
        // `WindowResized` is registered by `WindowPlugin`, which a head-less
        // app does not add; the reader must still have a queue to read from.
        app.add_message::<WindowResized>();
        app.init_resource::<BoardAnchors>()
            .init_resource::<BoardViewCache>()
            .init_resource::<BoardMetrics>()
            .init_resource::<crate::vfx::ReducedMotion>()
            .init_resource::<crate::hand::HandDrag>()
            .add_systems(
                OnEnter(AppScreen::Board),
                spawn_board.run_if(board_ready),
            )
            // …and again as soon as it *can* be built. `OnEnter` fires once,
            // in `StateTransition`, so a screen entered before the `Session`
            // exists would otherwise stay terrain-less for the whole game with
            // no way back short of replaying. The guard is "there is no board",
            // so this costs one empty query per frame and never double-spawns.
            .add_systems(
                Update,
                spawn_board
                    .in_set(AppSet::Render)
                    .before(BoardSet)
                    .run_if(in_state(AppScreen::Board))
                    .run_if(board_ready)
                    .run_if(board_missing),
            )
            .add_systems(
                Update,
                relayout_on_resize
                    .in_set(AppSet::Render)
                    .before(BoardSet)
                    .run_if(in_state(AppScreen::Board)),
            )
            .add_systems(
                Update,
                (refresh_view, sync_cells, sync_command, sync_header)
                    .chain()
                    .in_set(BoardSet)
                    .in_set(AppSet::Render)
                    .after(BridgeSet)
                    .run_if(in_state(AppScreen::Board))
                    .run_if(board_ready),
            )
            .add_systems(
                Update,
                (pulse_rings, update_anchors)
                    .in_set(AppSet::Render)
                    .after(BoardSet)
                    .run_if(in_state(AppScreen::Board)),
            )
            .add_systems(
                Update,
                apply_cover_fit
                    .in_set(AppSet::Render)
                    .after(BoardSet)
                    .run_if(in_state(AppScreen::Board))
                    .run_if(resource_exists::<Assets<Image>>),
            );
    }
}



// ============================================================
// Resources
// ============================================================

/// Run condition: the terrain is not on screen.
fn board_missing(roots: Query<(), With<BoardRoot>>) -> bool {
    roots.is_empty()
}

/// The last computed [`BoardView`]; `None` forces a full rebuild.
#[derive(Resource, Default)]
pub struct BoardViewCache(pub Option<BoardView>);

impl BoardViewCache {
    fn view(&self) -> Option<&BoardView> {
        self.0.as_ref()
    }
}

/// The resolved box sizes of one half (see [`geometry::half_metrics`]).
#[derive(Resource, Debug, Clone, Copy)]
pub struct BoardMetrics(pub HalfMetrics);

impl Default for BoardMetrics {
    fn default() -> Self {
        BoardMetrics(fit(l::WINDOW_W, l::WINDOW_H))
    }
}

/// The metrics a window of that logical size asks for.
fn fit(window_w: f32, window_h: f32) -> HalfMetrics {
    half_metrics(half_available_h(window_h), window_w)
}

/// Logical size of the primary window, falling back to the design size when
/// there is none (a head-less app).
fn window_size(windows: &Query<&Window, With<PrimaryWindow>>) -> Vec2 {
    windows
        .single()
        .map(|w| Vec2::new(w.resolution.width(), w.resolution.height()))
        .unwrap_or(Vec2::new(l::WINDOW_W, l::WINDOW_H))
}

/// Where the board's tiles sit on screen, so popovers and vfx can be anchored
/// on a unit without knowing anything about the board's entity tree.
///
/// Rectangles are in **logical** pixels, origin at the top-left of the window.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct BoardAnchors {
    /// Card instance id (and `captain_<player>` for the command cards) → tile.
    pub by_instance: HashMap<String, Rect>,
}

impl BoardAnchors {
    /// The tile of a unit / captain, if it is on screen.
    pub fn instance(&self, id: &str) -> Option<Rect> {
        self.by_instance.get(id).copied()
    }
}

// ============================================================
// Components
// ============================================================

/// Root of the whole board tree.
#[derive(Component)]
pub struct BoardRoot;

/// One of the twelve cells.
#[derive(Component)]
pub struct CellNode {
    pub slot: Slot,
    pub is_you: bool,
    content: Entity,
    veil: Entity,
}

#[derive(Component, Default)]
struct CellCache(Option<CellView>);

/// A command bar's captain card.
#[derive(Component)]
pub struct CaptainNode {
    pub player: PlayerId,
    pub is_you: bool,
    content: Entity,
    veil: Entity,
}

#[derive(Component, Default)]
struct CaptainCache(Option<CaptainCardView>);

/// A command bar's ship slot.
#[derive(Component)]
pub struct ShipNode {
    pub is_you: bool,
    content: Entity,
}

#[derive(Component, Default)]
struct ShipCache(Option<ShipView>);

/// A command bar's Volonté / counts column.
#[derive(Component)]
struct ResourceNode {
    is_you: bool,
    content: Entity,
}

#[derive(Component, Default)]
struct ResourceCache(Option<ResourceView>);

/// The top bar.
#[derive(Component)]
struct HeaderNode {
    content: Entity,
}

#[derive(Component, Default)]
struct HeaderCache(Option<HeaderView>);

/// Every box whose size is derived from [`BoardMetrics`], so one system can
/// re-fit the whole terrain when the window is resized.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum BoardBox {
    /// The battlefield column of a half (rows + captions + command bar).
    Column,
    /// A line of three slots.
    Row,
    /// A "LIGNE AVANT" caption.
    Label,
    /// A command bar.
    Command,
    /// One of the twelve cells.
    Cell,
    /// A command bar's captain card.
    Captain,
    /// A command bar's ship slot.
    Ship,
}

/// A ring that breathes (attack / impact).
#[derive(Component, Clone, Copy)]
struct RingPulse {
    color: Color,
}

/// Mock-up `.slot.unit{border:1px …}` — the resting hairline of a tile.
const CELL_BORDER: f32 = 1.0;
/// Mock-up `.slot.empty{border:1.5px dashed …}`, drawn as ticks instead, so the
/// cell's own border box is collapsed and the dashes sit on the edge.
const CELL_BORDER_EMPTY: f32 = 0.0;
/// Mock-up `.slot.sel{border:2px solid var(--gd)}`.
const CELL_BORDER_RING: f32 = 2.0;

// ============================================================
// Skeleton
// ============================================================

#[allow(clippy::too_many_arguments)]
fn spawn_board(
    mut commands: Commands,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    symbols: Res<SymbolFont>,
    session: Res<Session>,
    mode: Res<UiMode>,
    mut art: ResMut<ArtCache>,
    assets: Res<AssetServer>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cache: ResMut<BoardViewCache>,
    mut metrics: ResMut<BoardMetrics>,
) {
    let window = window_size(&windows);
    metrics.0 = fit(window.x, window.y);
    // The content is filled by the sync systems on the very next frame.
    cache.0 = None;

    let view = board_view(&session, &mode);
    let mut painter = Painter {
        commands: &mut commands,
        palette: &palette,
        fonts: &fonts,
        symbols: &symbols.0,
        art: &mut art,
        assets: &assets,
        metrics: metrics.0,
    };

    let root = painter
        .commands
        .spawn((
            Node {
                width: percent(100.0),
                height: percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            GlobalZIndex(l::Z_TERRAIN),
            Pickable::IGNORE,
            BoardRoot,
            DespawnOnExit(AppScreen::Board),
            Name::new("Board"),
        ))
        .id();

    spawn_glows(&mut painter, root);
    spawn_header(&mut painter, root);

    let area = painter
        .commands
        .spawn((
            Node {
                width: percent(100.0),
                flex_grow: 1.0,
                min_height: px(0.0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::clip(),
                ..default()
            },
            Pickable::IGNORE,
            Name::new("Terrain"),
            ChildOf(root),
        ))
        .id();

    spawn_half(&mut painter, area, &view, false);
    spawn_waterline(&mut painter, area);
    spawn_half(&mut painter, area, &view, true);

    // The hand owns the bottom strip; reserve it so the terrain never slides
    // under the footer — `l::HAND_H` is the sum of the footer's own boxes, so
    // YOUR command bar always stays above the cards.
    painter.commands.spawn((
        Node {
            width: percent(100.0),
            height: px(l::HAND_H),
            flex_shrink: 0.0,
            ..default()
        },
        Pickable::IGNORE,
        Name::new("HandReserve"),
        ChildOf(root),
    ));
}

/// The two blurred ambient discs of the mock-up (`.g1` / `.g2`).
///
/// CSS `filter: blur(75px)` has no Bevy UI equivalent; a radial gradient that
/// fades a saturated core to full transparency is the same picture, and it
/// costs one node instead of an off-screen pass.
fn spawn_glows(painter: &mut Painter, root: Entity) {
    let cool = painter.palette.glow_cool;
    let warm = painter.palette.glow_warm;
    let glow = |painter: &mut Painter, color: Color, name: &'static str, node: Node| {
        painter.commands.spawn((
            node,
            BackgroundGradient::from(RadialGradient::new(
                UiPosition::CENTER,
                RadialGradientShape::ClosestSide,
                vec![
                    ColorStop::new(color.with_alpha(0.50), percent(0.0)),
                    ColorStop::new(color.with_alpha(0.22), percent(55.0)),
                    ColorStop::new(color.with_alpha(0.0), percent(100.0)),
                ],
            )),
            GlobalZIndex(l::Z_GLOW),
            Pickable::IGNORE,
            Name::new(name),
            ChildOf(root),
        ));
    };
    glow(
        painter,
        cool,
        "GlowCool",
        Node {
            position_type: PositionType::Absolute,
            width: px(l::GLOW_D),
            height: px(l::GLOW_D),
            top: px(l::HEADER_H + 24.0),
            left: px(-l::GLOW_D * 0.34),
            border_radius: BorderRadius::MAX,
            ..default()
        },
    );
    glow(
        painter,
        warm,
        "GlowWarm",
        Node {
            position_type: PositionType::Absolute,
            width: px(l::GLOW_D),
            height: px(l::GLOW_D),
            bottom: px(l::HAND_H + 24.0),
            right: px(-l::GLOW_D * 0.34),
            border_radius: BorderRadius::MAX,
            ..default()
        },
    );
}

/// The top bar — mock-up `header`: brand left, the turn ring centred, the
/// "À toi" pill right, and **nothing else**.
///
/// The foe's hand / deck counts and its ship used to live here too; they belong
/// to the foe command bar (`counts_row` / the ship slot), which already draws
/// them, so the header no longer duplicates them. The foe's ship stays
/// inspectable by clicking that slot (`on_ship_clicked` → `ShipMenu`, which
/// shows the whole card frame).
///
/// The two "Fin de tour" / "Annuler" buttons are gone as well: the mock-up has
/// exactly one CTA row, in the footer, and [`crate::hand`] owns it.
fn spawn_header(painter: &mut Painter, root: Entity) {
    let palette = painter.palette;
    let deep = palette.bg_deep;

    let header = painter
        .commands
        .spawn((
            Node {
                width: percent(100.0),
                height: px(l::HEADER_H),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(10.0),
                padding: UiRect::horizontal(px(l::HEADER_PAD_X)),
                ..default()
            },
            // Mock-up: a vertical fade, no bottom rule — the terrain reads
            // continuously behind it.
            BackgroundGradient::from(LinearGradient::to_bottom(vec![
                ColorStop::new(deep.with_alpha(0.95), percent(0.0)),
                ColorStop::new(deep.with_alpha(0.40), percent(100.0)),
            ])),
            GlobalZIndex(l::Z_HEADER),
            Pickable::IGNORE,
            Name::new("Header"),
            ChildOf(root),
        ))
        .id();

    let content = painter
        .commands
        .spawn((
            Node {
                flex_grow: 1.0,
                height: percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(10.0),
                ..default()
            },
            Pickable::IGNORE,
            Name::new("HeaderContent"),
            ChildOf(header),
        ))
        .id();

    painter
        .commands
        .entity(header)
        .insert((HeaderNode { content }, HeaderCache::default()));
}

fn spawn_waterline(painter: &mut Painter, parent: Entity) {
    let water = painter.palette.waterline;
    let gold = painter.palette.gold;
    let strip = painter
        .commands
        .spawn((
            Node {
                width: percent(100.0),
                height: px(l::WATERLINE_H),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundGradient::from(LinearGradient::to_bottom(vec![
                ColorStop::new(Color::NONE, percent(0.0)),
                ColorStop::new(water.with_alpha(0.35), percent(50.0)),
                ColorStop::new(Color::NONE, percent(100.0)),
            ])),
            GlobalZIndex(l::Z_COMMAND),
            Pickable::IGNORE,
            Name::new("Waterline"),
            ChildOf(parent),
        ))
        .id();
    painter.commands.spawn((
        Node {
            width: percent(84.0),
            height: px(2.0),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundGradient::from(LinearGradient::to_right(vec![
            ColorStop::new(Color::NONE, percent(0.0)),
            ColorStop::new(gold.with_alpha(0.7), percent(25.0)),
            ColorStop::new(Color::WHITE.with_alpha(0.5), percent(50.0)),
            ColorStop::new(gold.with_alpha(0.7), percent(75.0)),
            ColorStop::new(Color::NONE, percent(100.0)),
        ])),
        Pickable::IGNORE,
        ChildOf(strip),
    ));
}

fn spawn_half(painter: &mut Painter, parent: Entity, view: &BoardView, is_you: bool) {
    let half = view.half(is_you);
    let metrics = painter.metrics;

    let root = painter
        .commands
        .spawn((
            Node {
                width: percent(100.0),
                flex_grow: 1.0,
                min_height: px(0.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                overflow: Overflow::clip(),
                ..default()
            },
            Pickable::IGNORE,
            Name::new(if is_you { "HalfYou" } else { "HalfFoe" }),
            ChildOf(parent),
        ))
        .id();

    // Ship-deck floor (yours is flipped, bow down) + the side's shade.
    let floor_box = metrics.half_box;
    painter.art(
        root,
        half.floor,
        Focus::CENTER,
        is_you,
        floor_box,
        widgets::NOMINAL_DECK,
    );
    // Mock-up `.shade.foe` / `.shade.you`: the third stop is the **side**'s
    // colour — pink over the foe, green over yours — never the faction's, so
    // playing the pirates does not paint your own half orange.
    let wash = painter.palette.wash(is_you);
    let shade = if is_you {
        LinearGradient::to_top(vec![
            ColorStop::new(Color::srgba(0.06, 0.16, 0.12, 0.55), percent(0.0)),
            ColorStop::new(Color::srgba(0.04, 0.05, 0.11, 0.40), percent(50.0)),
            ColorStop::new(wash, percent(100.0)),
        ])
    } else {
        LinearGradient::to_bottom(vec![
            ColorStop::new(Color::srgba(0.16, 0.06, 0.16, 0.55), percent(0.0)),
            ColorStop::new(Color::srgba(0.04, 0.05, 0.11, 0.40), percent(50.0)),
            ColorStop::new(wash, percent(100.0)),
        ])
    };
    painter.commands.spawn((
        fill_node(),
        BackgroundGradient::from(shade),
        GlobalZIndex(l::Z_SHADE),
        Pickable::IGNORE,
        ChildOf(root),
    ));

    let column = painter
        .commands
        .spawn((
            Node {
                width: px(metrics.battle_w),
                flex_direction: FlexDirection::Column,
                row_gap: px(metrics.gap),
                padding: UiRect::vertical(px(metrics.pad_y)),
                ..default()
            },
            BoardBox::Column,
            Pickable::IGNORE,
            ChildOf(root),
        ))
        .id();

    // Reading order, top to bottom: the foe's command bar is the farthest from
    // the waterline, yours is the closest to the hand.
    if is_you {
        spawn_row_label(painter, column, "Ligne avant");
        spawn_row(painter, column, &half.front, is_you);
        spawn_row_label(painter, column, "Ligne arrière");
        spawn_row(painter, column, &half.back, is_you);
        spawn_command(painter, column, half.player, is_you);
    } else {
        spawn_command(painter, column, half.player, is_you);
        spawn_row_label(painter, column, "Ligne arrière");
        spawn_row(painter, column, &half.back, is_you);
        spawn_row_label(painter, column, "Ligne avant");
        spawn_row(painter, column, &half.front, is_you);
    }
}

fn spawn_row_label(painter: &mut Painter, parent: Entity, label: &str) {
    let height = painter.metrics.label_h;
    let color = painter.palette.text_dim;
    // Mock-up `.rowlab` is body text, i.e. Poppins.
    let font = painter.fonts.poppins.clone();
    let row = painter
        .commands
        .spawn((
            Node {
                height: px(height),
                align_items: AlignItems::Center,
                padding: UiRect::left(px(3.0)),
                ..default()
            },
            BoardBox::Label,
            Pickable::IGNORE,
            ChildOf(parent),
        ))
        .id();
    painter.commands.spawn((
        style::caption(
            label.to_uppercase(),
            &font,
            l::FS_TINY,
            color,
            l::FS_TINY * 0.3,
        ),
        ChildOf(row),
    ));
}

fn spawn_row(painter: &mut Painter, parent: Entity, cells: &[CellView], is_you: bool) {
    let metrics = painter.metrics;
    let row = painter
        .commands
        .spawn((
            Node {
                width: percent(100.0),
                height: px(metrics.slot_h),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                column_gap: px(metrics.gap_x()),
                ..default()
            },
            BoardBox::Row,
            GlobalZIndex(l::Z_ROW),
            Pickable::IGNORE,
            ChildOf(parent),
        ))
        .id();

    for cell in cells {
        spawn_cell(painter, row, cell.slot, is_you);
    }
}

fn spawn_cell(painter: &mut Painter, parent: Entity, slot: Slot, is_you: bool) {
    let bg = painter.palette.bg_slot;
    let ink = painter.palette.ink;
    let metrics = painter.metrics;
    let radius = metrics.chrome(l::SLOT_RADIUS);
    let content = painter.commands.spawn_empty().id();
    let veil = painter.commands.spawn_empty().id();

    let cell = painter
        .commands
        .spawn((
            Node {
                width: px(metrics.slot_w),
                height: percent(100.0),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                // Mock-up: `.slot.unit` is a 1 px hairline and 2 px is the
                // *selected* weight only. The box is rewritten per state by
                // `paint_decor`, which also collapses it to zero on an empty
                // slot so the dashed placeholder is not inset by a border the
                // mock-up does not draw.
                border: UiRect::all(px(metrics.chrome(CELL_BORDER))),
                border_radius: BorderRadius::all(px(radius)),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor::all(ink),
            BoardBox::Cell,
            CellNode {
                slot,
                is_you,
                content,
                veil,
            },
            CellCache::default(),
            Name::new(format!("Cell{slot:?}")),
            ChildOf(parent),
        ))
        .id();

    painter.commands.entity(content).insert((
        Node {
            overflow: Overflow::clip(),
            border_radius: BorderRadius::all(px(radius)),
            ..fill_node()
        },
        Pickable::IGNORE,
        ChildOf(cell),
    ));
    painter.commands.entity(veil).insert((
        fill_node(),
        BackgroundColor(Color::NONE),
        Pickable::IGNORE,
        ChildOf(cell),
    ));
    painter
        .commands
        .entity(cell)
        .observe(on_cell_clicked)
        .observe(on_cell_dropped);
}

fn spawn_command(painter: &mut Painter, parent: Entity, player: PlayerId, is_you: bool) {
    let metrics = painter.metrics;
    let side = painter.palette.side(is_you);
    let cyan = painter.palette.def;
    let slot_bg = painter.palette.bg_slot;

    let bar = painter
        .commands
        .spawn((
            Node {
                width: percent(100.0),
                height: px(metrics.cmd_h),
                flex_direction: FlexDirection::Row,
                column_gap: px(metrics.chrome(l::CMD_GAP)),
                ..default()
            },
            BoardBox::Command,
            GlobalZIndex(l::Z_COMMAND),
            Pickable::IGNORE,
            Name::new(if is_you { "CommandYou" } else { "CommandFoe" }),
            ChildOf(parent),
        ))
        .id();

    // --- captain card ---
    let cap_content = painter.commands.spawn_empty().id();
    let cap_veil = painter.commands.spawn_empty().id();
    let captain = painter
        .commands
        .spawn((
            Node {
                width: px(metrics.captain_w),
                height: percent(100.0),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                border: UiRect::all(px(2.0)),
                border_radius: BorderRadius::all(px(12.0)),
                ..default()
            },
            BackgroundColor(slot_bg),
            BorderColor::all(side),
            BoardBox::Captain,
            CaptainNode {
                player,
                is_you,
                content: cap_content,
                veil: cap_veil,
            },
            CaptainCache::default(),
            Name::new("CaptainCard"),
            ChildOf(bar),
        ))
        .id();
    painter.commands.entity(cap_content).insert((
        fill_node(),
        Pickable::IGNORE,
        ChildOf(captain),
    ));
    painter.commands.entity(cap_veil).insert((
        fill_node(),
        BackgroundColor(Color::NONE),
        Pickable::IGNORE,
        ChildOf(captain),
    ));
    painter
        .commands
        .entity(captain)
        .observe(on_captain_clicked);

    // --- ship slot ---
    let ship_content = painter.commands.spawn_empty().id();
    let ship = painter
        .commands
        .spawn((
            Node {
                width: px(metrics.ship_w),
                height: percent(100.0),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                border: UiRect::all(px(1.0)),
                border_radius: BorderRadius::all(px(12.0)),
                ..default()
            },
            BackgroundColor(slot_bg),
            BorderColor::all(cyan.with_alpha(0.45)),
            BoardBox::Ship,
            ShipNode {
                is_you,
                content: ship_content,
            },
            ShipCache::default(),
            Name::new("ShipSlot"),
            ChildOf(bar),
        ))
        .id();
    painter.commands.entity(ship_content).insert((
        fill_node(),
        Pickable::IGNORE,
        ChildOf(ship),
    ));
    painter.commands.entity(ship).observe(on_ship_clicked);

    // --- volonté / counts ---
    let res_content = painter.commands.spawn_empty().id();
    let res = painter
        .commands
        .spawn((
            Node {
                flex_grow: 1.0,
                height: percent(100.0),
                min_width: px(0.0),
                ..default()
            },
            ResourceNode {
                is_you,
                content: res_content,
            },
            ResourceCache::default(),
            Pickable::IGNORE,
            Name::new("Resources"),
            ChildOf(bar),
        ))
        .id();
    painter.commands.entity(res_content).insert((
        Node {
            width: percent(100.0),
            height: percent(100.0),
            ..default()
        },
        Pickable::IGNORE,
        ChildOf(res),
    ));
}

// ============================================================
// Sync
// ============================================================

fn refresh_view(session: Res<Session>, mode: Res<UiMode>, mut cache: ResMut<BoardViewCache>) {
    if !(session.is_changed() || mode.is_changed() || cache.0.is_none()) {
        return;
    }
    let next = board_view(&session, &mode);
    if cache.0.as_ref() != Some(&next) {
        cache.0 = Some(next);
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_cells(
    mut commands: Commands,
    cache: Res<BoardViewCache>,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    mut art: ResMut<ArtCache>,
    assets: Res<AssetServer>,
    metrics: Res<BoardMetrics>,
    symbols: Res<SymbolFont>,
    mut cells: Query<(Entity, &CellNode, &mut CellCache)>,
    mut tints: Query<(&mut BorderColor, &mut BackgroundColor)>,
    mut boxes: Query<&mut Node>,
) {
    // A resize re-fits every box, so the chrome inside a tile (badges, HP bar
    // inset, dashes) has to be repainted even though the *content* is the same.
    let refit = metrics.is_changed();
    if !(cache.is_changed() || refit) {
        return;
    }
    let Some(view) = cache.view() else { return };
    let palette = &*palette;
    let mut painter = Painter {
        commands: &mut commands,
        palette,
        fonts: &fonts,
        symbols: &symbols.0,
        art: &mut art,
        assets: &assets,
        metrics: metrics.0,
    };

    for (entity, node, mut cached) in &mut cells {
        let Some(next) = view.half(node.is_you).cell(node.slot) else {
            continue;
        };
        let content_changed =
            refit || cached.0.as_ref().is_none_or(|c| c.content != next.content);
        let decor_changed = refit || cached.0.as_ref().is_none_or(|c| c.decor != next.decor);
        if content_changed {
            painter
                .commands
                .entity(node.content)
                .despawn_related::<Children>();
            painter.cell_content(node.content, next);
        }
        if content_changed || decor_changed {
            let (resting, resting_w) = match &next.content {
                CellContent::Unit(unit) => (unit.border, CELL_BORDER),
                CellContent::Captain(_) => (palette.foe, CELL_BORDER),
                // Mock-up `.slot.empty` is dashed and nothing else: the solid
                // frame would fight the dashes `empty_cell` draws, and its box
                // would inset them by a border the mock-up has no room for.
                CellContent::Empty => (Color::NONE, CELL_BORDER_EMPTY),
            };
            // A ring is the mock-up's `.slot.sel` weight; everything else rests
            // on the hairline (or on nothing at all).
            let weight = if next.decor.ring == Ring::None {
                resting_w
            } else {
                CELL_BORDER_RING
            };
            paint_decor(
                painter.commands,
                &mut tints,
                &mut boxes,
                entity,
                node.veil,
                next.decor.ring,
                next.decor.dimmed,
                resting,
                Some(metrics.0.chrome(weight)),
                palette,
            );
        }
        if content_changed || decor_changed {
            cached.0 = Some(next.clone());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_command(
    mut commands: Commands,
    cache: Res<BoardViewCache>,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    mut art: ResMut<ArtCache>,
    assets: Res<AssetServer>,
    metrics: Res<BoardMetrics>,
    symbols: Res<SymbolFont>,
    mut captains: Query<(Entity, &CaptainNode, &mut CaptainCache)>,
    mut ships: Query<(Entity, &ShipNode, &mut ShipCache)>,
    mut resources: Query<(&ResourceNode, &mut ResourceCache)>,
    mut tints: Query<(&mut BorderColor, &mut BackgroundColor)>,
    mut boxes: Query<&mut Node>,
) {
    let refit = metrics.is_changed();
    if !(cache.is_changed() || refit) {
        return;
    }
    let Some(view) = cache.view() else { return };
    let palette = &*palette;
    let mut painter = Painter {
        commands: &mut commands,
        palette,
        fonts: &fonts,
        symbols: &symbols.0,
        art: &mut art,
        assets: &assets,
        metrics: metrics.0,
    };

    for (entity, node, mut cached) in &mut captains {
        let next = &view.half(node.is_you).command.captain;
        if !refit && cached.0.as_ref() == Some(next) {
            continue;
        }
        let content_changed = refit
            || cached
                .0
                .as_ref()
                .is_none_or(|c| CaptainCardView { decor: next.decor, ..c.clone() } != *next);
        if content_changed {
            painter
                .commands
                .entity(node.content)
                .despawn_related::<Children>();
            painter.captain_content(node.content, next);
        }
        paint_decor(
            painter.commands,
            &mut tints,
            &mut boxes,
            entity,
            node.veil,
            next.decor.ring,
            next.decor.dimmed,
            palette.side(node.is_you),
            None,
            palette,
        );
        cached.0 = Some(next.clone());
    }

    for (entity, node, mut cached) in &mut ships {
        let next = &view.half(node.is_you).command.ship;
        if !refit && cached.0.as_ref() == Some(next) {
            continue;
        }
        painter
            .commands
            .entity(node.content)
            .despawn_related::<Children>();
        painter.ship_content(node.content, next);
        // Mock-up `.shipslot.empty{border-style:dashed}` — the empty slot
        // *swaps* the solid cyan edge for dashes, it does not wear both. Bevy
        // has no dashed border, so `ship_content` paints the ticks and the
        // solid box is collapsed here, exactly like an empty cell's.
        let occupied = next.name.is_some();
        if let Ok((mut border, _)) = tints.get_mut(entity) {
            let color = if occupied {
                palette.def.with_alpha(0.45)
            } else {
                Color::NONE
            };
            let want = BorderColor::all(color);
            if *border != want {
                *border = want;
            }
        }
        if let Ok(mut box_) = boxes.get_mut(entity) {
            let want = UiRect::all(px(if occupied { metrics.0.chrome(1.0) } else { 0.0 }));
            if box_.border != want {
                box_.border = want;
            }
        }
        cached.0 = Some(next.clone());
    }

    for (node, mut cached) in &mut resources {
        let next = view.half(node.is_you).command.resources;
        if !refit && cached.0 == Some(next) {
            continue;
        }
        painter
            .commands
            .entity(node.content)
            .despawn_related::<Children>();
        painter.resources_content(node.content, &next);
        cached.0 = Some(next);
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_header(
    mut commands: Commands,
    cache: Res<BoardViewCache>,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    symbols: Res<SymbolFont>,
    mut art: ResMut<ArtCache>,
    assets: Res<AssetServer>,
    metrics: Res<BoardMetrics>,
    mut headers: Query<(&HeaderNode, &mut HeaderCache)>,
) {
    let refit = metrics.is_changed();
    if !(cache.is_changed() || refit) {
        return;
    }
    let Some(view) = cache.view() else { return };
    let palette = &*palette;
    let mut painter = Painter {
        commands: &mut commands,
        palette,
        fonts: &fonts,
        symbols: &symbols.0,
        art: &mut art,
        assets: &assets,
        metrics: metrics.0,
    };

    for (node, mut cached) in &mut headers {
        if !refit && cached.0.as_ref() == Some(&view.header) {
            continue;
        }
        painter
            .commands
            .entity(node.content)
            .despawn_related::<Children>();
        painter.header_content(node.content, &view.header);
        cached.0 = Some(view.header.clone());
    }
}

/// Re-fit the whole terrain when the window is resized.
///
/// Every box whose size comes from [`BoardMetrics`] carries a [`BoardBox`], so
/// one pass over that query is the entire relayout; bumping `BoardMetrics`
/// itself is what makes the sync systems repaint the content whose fixed-pixel
/// chrome (badges, HP-bar inset, dashes) is scaled too.
fn relayout_on_resize(
    mut resized: MessageReader<WindowResized>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut metrics: ResMut<BoardMetrics>,
    boxes: Query<(&BoardBox, &mut Node)>,
) {
    if resized.read().count() == 0 {
        return;
    }
    let window = window_size(&windows);
    // A minimised window reports (0, 0), which `half_metrics` reads as "no
    // constraint" and answers at scale 1 — a full repaint on the way down and
    // another on the way back up, for a board nobody can see. Keep what we had.
    if !(window.x > 0.0 && window.y > 0.0) {
        return;
    }
    let next = fit(window.x, window.y);
    if next == metrics.0 {
        return;
    }
    metrics.0 = next;
    apply_metrics(next, boxes);
}

/// Write the resolved metrics onto every [`BoardBox`].
fn apply_metrics(m: HalfMetrics, boxes: Query<(&BoardBox, &mut Node)>) {
    for (kind, mut node) in boxes {
        match kind {
            BoardBox::Column => {
                node.width = px(m.battle_w);
                node.row_gap = px(m.gap);
                node.padding = UiRect::vertical(px(m.pad_y));
            }
            BoardBox::Row => {
                node.height = px(m.slot_h);
                node.column_gap = px(m.gap_x());
            }
            BoardBox::Label => node.height = px(m.label_h),
            BoardBox::Command => {
                node.height = px(m.cmd_h);
                node.column_gap = px(m.chrome(l::CMD_GAP));
            }
            BoardBox::Cell => {
                node.width = px(m.slot_w);
                node.border_radius = BorderRadius::all(px(m.chrome(l::SLOT_RADIUS)));
            }
            BoardBox::Captain => node.width = px(m.captain_w),
            BoardBox::Ship => node.width = px(m.ship_w),
        }
    }
}

/// Re-tint a cell / captain card: ring, resting border and dim veil.
///
/// `weight` is the border *width* the state asks for, in design pixels, or
/// `None` to leave the box alone (the captain card's frame is constant).
#[allow(clippy::too_many_arguments)]
fn paint_decor(
    commands: &mut Commands,
    tints: &mut Query<(&mut BorderColor, &mut BackgroundColor)>,
    boxes: &mut Query<&mut Node>,
    entity: Entity,
    veil: Entity,
    ring: Ring,
    dimmed: bool,
    resting: Color,
    weight: Option<f32>,
    palette: &Palette,
) {
    let border = ring.color(palette).unwrap_or(resting);
    if let Ok((mut color, _)) = tints.get_mut(entity) {
        let next = BorderColor::all(border);
        if *color != next {
            *color = next;
        }
    }
    if let (Some(weight), Ok(mut node)) = (weight, boxes.get_mut(entity)) {
        let next = UiRect::all(px(weight));
        if node.border != next {
            node.border = next;
        }
    }
    if let Ok((_, mut background)) = tints.get_mut(veil) {
        let color = if dimmed {
            palette.bg_deep.with_alpha(0.55)
        } else {
            Color::NONE
        };
        if background.0 != color {
            background.0 = color;
        }
    }
    if ring.pulses() {
        commands.entity(entity).insert(RingPulse { color: border });
    } else {
        commands.entity(entity).remove::<RingPulse>();
    }
}

// ============================================================
// Per-frame polish
// ============================================================

/// Attack / impact rings breathe.
///
/// Reduced motion parks them at full strength: this and the hand's playable
/// pips are the two never-ending loops on screen, so leaving them flashing
/// would make the module's promise ("shorten every animation and drop the ones
/// that move the whole screen") false in the one place it matters most.
fn pulse_rings(
    time: Res<Time>,
    reduced: Res<crate::vfx::ReducedMotion>,
    mut rings: Query<(&RingPulse, &mut BorderColor)>,
) {
    let alpha = if reduced.enabled {
        1.0
    } else {
        0.45 + 0.55 * ((time.elapsed_secs() * 5.0).sin() * 0.5 + 0.5)
    };
    for (pulse, mut border) in &mut rings {
        // An unchanged write would still trip change detection every frame,
        // which is exactly the cost reduced motion is asked to remove.
        let next = BorderColor::all(pulse.color.with_alpha(alpha));
        if *border != next {
            *border = next;
        }
    }
}

/// Crop every marked illustration "cover"-style once its texture size is known.
fn apply_cover_fit(
    images: Res<Assets<Image>>,
    parents: Query<&ComputedNode>,
    mut fitted: Query<(&CoverFit, &ImageNode, &ChildOf, &mut Node)>,
) {
    for (fit, image, child_of, mut node) in &mut fitted {
        let Ok(parent) = parents.get(child_of.parent()) else {
            continue;
        };
        let Some(texture) = images.get(&image.image) else {
            continue;
        };
        let scale = if parent.inverse_scale_factor > 0.0 {
            parent.inverse_scale_factor
        } else {
            1.0
        };
        let box_size = parent.size() * scale;
        let size = texture.size_f32();
        let rect = cover(box_size.x, box_size.y, size.x, size.y, fit.focus);
        let (width, height, left, top) = (
            px(rect.width),
            px(rect.height),
            px(rect.left),
            px(rect.top),
        );
        if node.width != width || node.height != height || node.left != left || node.top != top {
            node.width = width;
            node.height = height;
            node.left = left;
            node.top = top;
            node.right = Val::Auto;
            node.bottom = Val::Auto;
        }
    }
}

/// Publish where every tile is, for popovers and vfx.
fn update_anchors(
    mut anchors: ResMut<BoardAnchors>,
    cells: Query<(&CellCache, &ComputedNode, &UiGlobalTransform)>,
    captains: Query<(&CaptainNode, &ComputedNode, &UiGlobalTransform)>,
) {
    let anchors = anchors.bypass_change_detection();
    anchors.by_instance.clear();

    for (cached, computed, transform) in &cells {
        let rect = logical_rect(computed, transform);
        if let Some(view) = &cached.0 {
            match &view.content {
                CellContent::Unit(unit) => {
                    anchors.by_instance.insert(unit.instance_id.clone(), rect);
                }
                CellContent::Captain(token) => {
                    anchors
                        .by_instance
                        .insert(crate::selection::captain_key(token.player), rect);
                }
                CellContent::Empty => {}
            }
        }
    }

    for (node, computed, transform) in &captains {
        let key = crate::selection::captain_key(node.player);
        anchors
            .by_instance
            .entry(key)
            .or_insert_with(|| logical_rect(computed, transform));
    }
}

fn logical_rect(computed: &ComputedNode, transform: &UiGlobalTransform) -> Rect {
    let scale = if computed.inverse_scale_factor > 0.0 {
        computed.inverse_scale_factor
    } else {
        1.0
    };
    Rect::from_center_size(transform.translation * scale, computed.size() * scale)
}

// ============================================================
// Clicks
// ============================================================

/// Every board observer takes `Option<Res<Session>>` and returns early when it
/// is gone, exactly like the hand's `end_turn_clicked`.
///
/// A missing resource in an observer's parameters is a **system-param
/// validation error**, not a skip, so a non-optional `Res<Session>` would turn
/// a click that lands after the session was dropped into a crash on one side
/// and a silent no-op on the other — for the very same gesture.
fn on_cell_clicked(
    click: On<Pointer<Click>>,
    cells: Query<&CellNode>,
    session: Option<Res<Session>>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut dispatch: MessageWriter<DispatchAction>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let Some(session) = session else { return };
    let Ok(cell) = cells.get(click.entity) else {
        return;
    };
    let ai = session.ai_player();
    let player = if cell.is_you { session.human } else { ai };
    let ps = session.state.player(player);

    let command = if ps.captain.flipped && ps.captain.slot == Some(cell.slot) {
        // The verso token answers to the captain handler, like `renderLine`.
        on_captain_click(&mode, &session.valid, ai, player)
    } else {
        let occupant = ps
            .board
            .get(cell.slot)
            .and_then(|id| session.state.card(id))
            .map(|card| (card.instance_id.as_str(), card.def_id.as_str()));
        on_cell_click(
            &mode,
            &session.valid,
            ai,
            cell.slot,
            cell.is_you,
            occupant,
        )
    };
    submit(command, &mut mode, &mut selected, &mut dispatch);
}

fn on_captain_clicked(
    click: On<Pointer<Click>>,
    captains: Query<&CaptainNode>,
    session: Option<Res<Session>>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut dispatch: MessageWriter<DispatchAction>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let Some(session) = session else { return };
    let Ok(captain) = captains.get(click.entity) else {
        return;
    };
    let ai = session.ai_player();
    let command = on_captain_click(&mode, &session.valid, ai, captain.player);
    submit(command, &mut mode, &mut selected, &mut dispatch);
}

fn on_ship_clicked(
    click: On<Pointer<Click>>,
    ships: Query<&ShipNode>,
    session: Option<Res<Session>>,
    mut mode: ResMut<UiMode>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let Some(session) = session else { return };
    let Ok(ship) = ships.get(click.entity) else {
        return;
    };
    let player = if ship.is_you {
        session.human
    } else {
        session.ai_player()
    };
    if let Some(instance_id) = session.state.player(player).active_ship.clone() {
        *mode = UiMode::ShipMenu {
            instance_id,
            is_you: ship.is_you,
        };
    }
}

/// A hand card was dropped on a cell — TS `BoardSlot.onDrop` → `act()`.
///
/// The drag already put the [`UiMode`] in `SelectingSlot` /
/// `SelectingEquipTarget` (`hand_card_drag_started`), so finishing the gesture
/// is literally the click path: the drop is routed through the exact same
/// [`on_cell_click`] the pointer would have reached, which is why a drop onto
/// an illegal cell is swallowed rather than dispatched.
///
/// **Which** card was released matters. In the DOM the gesture cannot even
/// begin anywhere but a playable character / object
/// (`draggable={canPlay && dragType}`, and board slots carry no `draggable` at
/// all), so `onDrop` can only ever run after a legal hand drag. Bevy makes
/// every observed node a drag source, so the check is explicit:
/// [`HandDrag::carries`] must recognise the dropped entity, or the release is
/// not the gesture the armed selection belongs to and completing it would
/// deploy — or attack with — something else entirely.
fn on_cell_dropped(
    drop: On<Pointer<DragDrop>>,
    cells: Query<&CellNode>,
    session: Option<Res<Session>>,
    mut drag: ResMut<crate::hand::HandDrag>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut dispatch: MessageWriter<DispatchAction>,
) {
    if drop.button != PointerButton::Primary {
        return;
    }
    if !drag.carries(drop.dropped) {
        return;
    }
    let Some(session) = session else { return };
    let Ok(cell) = cells.get(drop.entity) else {
        return;
    };
    // Whatever the cell makes of it, the gesture ended here: `DragEnd` (which
    // fires straight after) must not also cancel the selection.
    drag.consumed = true;
    let ai = session.ai_player();
    let player = if cell.is_you { session.human } else { ai };
    let ps = session.state.player(player);
    let occupant = ps
        .board
        .get(cell.slot)
        .and_then(|id| session.state.card(id))
        .map(|card| (card.instance_id.as_str(), card.def_id.as_str()));
    let command = on_cell_drop(
        &mode,
        &session.valid,
        ai,
        cell.slot,
        cell.is_you,
        occupant,
    );
    submit(command, &mut mode, &mut selected, &mut dispatch);
}

/// Apply a [`UiCommand`] and forward the action it produced to the bridge.
fn submit(
    command: UiCommand,
    mode: &mut UiMode,
    selected: &mut SelectedHandCard,
    dispatch: &mut MessageWriter<DispatchAction>,
) {
    if let Some(action) = apply_ui_command(command, mode, selected) {
        dispatch.write(DispatchAction(action));
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection::SelectionPlugin;
    use tcgop_engine::types::GameAction;

    /// The plugin must build, and every one of its systems must *initialise*,
    /// in an app with no window, no assets and no game — that is what catches a
    /// conflicting query or a bad system signature without a display.
    #[test]
    fn the_plugin_initialises_head_lessly_and_draws_nothing() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<AppScreen>()
            .insert_resource(Palette::default())
            .insert_resource(Fonts::default())
            .init_resource::<ArtCache>()
            .add_plugins((SelectionPlugin, BoardPlugin));
        app.update();
        app.update();

        assert!(app.world().get_resource::<BoardAnchors>().is_some());
        assert!(
            app.world().resource::<BoardViewCache>().0.is_none(),
            "no session, no view"
        );
        let mut roots = app.world_mut().query_filtered::<Entity, With<BoardRoot>>();
        assert_eq!(
            roots.iter(app.world()).count(),
            0,
            "the terrain only exists on the board screen, with an asset server"
        );
    }

    /// The real thing, without a window: a session, an asset server, the state
    /// switched to `Board` — the skeleton must spawn and every painter must run
    /// (this is what catches a bad bundle or a panicking widget).
    #[test]
    fn the_terrain_spawns_and_fills_itself_from_a_session() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(bevy::state::app::StatesPlugin)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .init_asset::<Image>()
            .init_state::<AppScreen>()
            .insert_resource(Palette::default())
            .insert_resource(Fonts::default())
            .init_resource::<ArtCache>()
            .add_plugins((crate::bridge::BridgePlugin, SelectionPlugin, BoardPlugin))
            .insert_resource(crate::bridge::testkit::session(3));

        app.world_mut()
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::Board);
        app.update();
        app.update();

        assert_eq!(count::<CellNode>(&mut app), 12, "six slots per side");
        assert_eq!(count::<CaptainNode>(&mut app), 2);
        assert_eq!(count::<ShipNode>(&mut app), 2);
        assert_eq!(count::<HeaderNode>(&mut app), 1);

        // The sync pass ran: every cell cached what it drew…
        let mut cells = app
            .world_mut()
            .query::<(&CellNode, &CellCache, &Children)>();
        let mut filled = 0;
        for (_, cache, children) in cells.iter(app.world()) {
            assert!(cache.0.is_some(), "every cell must have been synced");
            assert_eq!(children.len(), 2, "content root + dim veil");
            filled += 1;
        }
        assert_eq!(filled, 12);

        // …and the empty tiles have their slot caption underneath.
        let view = app.world().resource::<BoardViewCache>().0.clone();
        let view = view.expect("the view is computed as soon as the board opens");
        assert!(view.you.front.iter().all(|c| c.content == CellContent::Empty));
        assert!(view.header.can_end_turn);
    }

    /// Only the tile that changed is rebuilt: every other tile keeps the exact
    /// same child entities, so nothing flickers.
    #[test]
    fn a_state_change_only_rebuilds_the_tiles_that_changed() {
        let mut session = crate::bridge::testkit::session(11);
        assert!(
            crate::bridge::testkit::advance(&mut session, 200, |s| s
                .valid
                .iter()
                .any(|a| matches!(a, GameAction::DeployCharacter { .. }))),
            "the human must eventually afford a character"
        );
        let (instance_id, slot) = session
            .valid
            .iter()
            .find_map(|a| match a {
                GameAction::DeployCharacter { instance_id, slot } => {
                    Some((instance_id.clone(), *slot))
                }
                _ => None,
            })
            .expect("checked above");

        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(bevy::state::app::StatesPlugin)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .init_asset::<Image>()
            .init_state::<AppScreen>()
            .insert_resource(Palette::default())
            .insert_resource(Fonts::default())
            .init_resource::<ArtCache>()
            .add_plugins((crate::bridge::BridgePlugin, SelectionPlugin, BoardPlugin))
            .insert_resource(session);
        app.world_mut()
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::Board);
        app.update();
        app.update();

        let before = tile_children(&mut app);
        app.world_mut()
            .write_message(DispatchAction(GameAction::DeployCharacter {
                instance_id,
                slot,
            }));
        app.update();
        let after = tile_children(&mut app);

        assert_ne!(
            before[&(true, slot)],
            after[&(true, slot)],
            "the tile that received the unit is repainted"
        );
        for (key, children) in &before {
            if *key == (true, slot) {
                continue;
            }
            assert_eq!(
                children, &after[key],
                "tile {key:?} was rebuilt although nothing about it changed"
            );
        }
    }

    /// `(is_you, slot)` → the entities currently drawing that tile.
    fn tile_children(app: &mut App) -> HashMap<(bool, Slot), Vec<Entity>> {
        let mut cells = app.world_mut().query::<&CellNode>();
        let nodes: Vec<(bool, Slot, Entity)> = cells
            .iter(app.world())
            .map(|c| (c.is_you, c.slot, c.content))
            .collect();
        nodes
            .into_iter()
            .map(|(is_you, slot, content)| {
                let children = app
                    .world()
                    .get::<Children>(content)
                    .map(|c| c.iter().collect::<Vec<_>>())
                    .unwrap_or_default();
                ((is_you, slot), children)
            })
            .collect()
    }

    fn count<C: Component>(app: &mut App) -> usize {
        let mut query = app.world_mut().query_filtered::<Entity, With<C>>();
        query.iter(app.world()).count()
    }

    #[test]
    fn the_metrics_resource_fits_the_default_window() {
        let metrics = BoardMetrics::default().0;
        assert!(metrics.slot_h > 0.0 && metrics.cmd_h > 0.0);
        assert!(metrics.total_h() <= half_available_h(l::WINDOW_H) + 0.01);
    }

    /// The terrain and the footer must tile the window exactly: the reserve the
    /// board leaves for the hand is the footer's own height, so YOUR command
    /// bar is never behind the cards.
    ///
    /// The mock-up's `footer` is `.hand` + `.cta` and nothing else — the log
    /// and the hints float above it — so the reserve is those two boxes plus
    /// the padding, and no more.
    #[test]
    fn the_footer_reserve_is_exactly_the_footer_height() {
        let footer = l::FOOTER_PAD_TOP
            + (l::HAND_CARD_H + l::HAND_FAN_LIFT_MAX)
            + l::FOOTER_ROW_GAP
            + l::BUTTON_H
            + l::FOOTER_PAD_BOTTOM;
        assert!((l::HAND_H - footer).abs() < 1e-3, "{} vs {footer}", l::HAND_H);

        // …and it leaves the terrain the lion's share of the window, the way
        // the mock-up's 160 px footer does on an 840 px phone.
        let share = std::hint::black_box(l::HAND_H) / l::WINDOW_H;
        assert!(share < 0.28, "the footer eats {:.0}% of the window", share * 100.0);

        // …and there is still room for two halves above it.
        assert!(half_available_h(l::WINDOW_H) > 0.0);
        let metrics = BoardMetrics::default().0;
        assert!(l::HEADER_H + 2.0 * metrics.total_h() + l::WATERLINE_H + l::HAND_H <= l::WINDOW_H + 0.01);
    }

    /// Exactly one "Fin de tour" and one "Annuler": the mock-up has a single
    /// `.cta` row, in the footer, and the header carries no button at all.
    #[test]
    fn the_header_spawns_no_buttons() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(bevy::state::app::StatesPlugin)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .init_asset::<Image>()
            .init_state::<AppScreen>()
            .insert_resource(Palette::default())
            .insert_resource(Fonts::default())
            .init_resource::<ArtCache>()
            .add_plugins((crate::bridge::BridgePlugin, SelectionPlugin, BoardPlugin))
            .insert_resource(crate::bridge::testkit::session(5));
        app.world_mut()
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::Board);
        app.update();
        app.update();

        assert_eq!(
            count::<Button>(&mut app),
            0,
            "the board layer owns no button — the footer's CTA row is the only one"
        );
    }

    /// Resizing re-fits every box that came from the metrics.
    #[test]
    fn a_resize_refits_the_terrain() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(bevy::state::app::StatesPlugin)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .init_asset::<Image>()
            .init_state::<AppScreen>()
            .insert_resource(Palette::default())
            .insert_resource(Fonts::default())
            .init_resource::<ArtCache>()
            .add_plugins((crate::bridge::BridgePlugin, SelectionPlugin, BoardPlugin))
            .insert_resource(crate::bridge::testkit::session(5));
        app.world_mut()
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::Board);
        app.update();
        app.update();

        let before = app.world().resource::<BoardMetrics>().0;
        // No window in a head-less app, so `fit` falls back to the design size:
        // apply a taller one directly, the way the resize handler would.
        let taller = fit(l::WINDOW_W, l::WINDOW_H * 1.4);
        assert!(taller.slot_h > before.slot_h);
        assert!(taller.battle_w > before.battle_w);

        app.world_mut().resource_mut::<BoardMetrics>().0 = taller;
        let mut boxes = app.world_mut().query::<(&BoardBox, &mut Node)>();
        let query = boxes.query_mut(app.world_mut());
        apply_metrics(taller, query);

        let mut cells = app.world_mut().query::<(&BoardBox, &Node)>();
        let widths: Vec<Val> = cells
            .iter(app.world())
            .filter(|(kind, _)| **kind == BoardBox::Cell)
            .map(|(_, node)| node.width)
            .collect();
        assert_eq!(widths.len(), 12);
        assert!(widths.iter().all(|w| *w == px(taller.slot_w)));
    }
}

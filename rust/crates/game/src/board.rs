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

pub mod geometry;
pub mod interaction;
pub mod model;
pub mod style;
pub mod widgets;

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use tcgop_engine::types::{GameAction, PlayerId, Slot};

use crate::app::{AppScreen, AppSet, Fonts, Palette, configure_pipeline, layout as l};
use crate::art::{ArtCache, Focus};
use crate::bridge::{BridgeSet, DispatchAction, Session};
use crate::selection::{
    SelectedHandCard, UiCommand, UiMode, on_captain_click, on_cell_click,
};

use geometry::{BATTLE_W, HalfMetrics, cover, half_available_h, half_metrics};
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
        app.init_resource::<BoardAnchors>()
            .init_resource::<BoardViewCache>()
            .init_resource::<BoardMetrics>()
            .add_systems(
                OnEnter(AppScreen::Board),
                spawn_board.run_if(board_ready),
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

/// The board can only be built with a game **and** an asset server (a head-less
/// app has neither).
fn board_ready(session: Option<Res<Session>>, assets: Option<Res<AssetServer>>) -> bool {
    session.is_some() && assets.is_some()
}

// ============================================================
// Resources
// ============================================================

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
        BoardMetrics(half_metrics(half_available_h(l::WINDOW_H)))
    }
}

/// Where the board's tiles sit on screen, so popovers and vfx can be anchored
/// on a unit without knowing anything about the board's entity tree.
///
/// Rectangles are in **logical** pixels, origin at the top-left of the window.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct BoardAnchors {
    /// Card instance id (and `captain_<player>` for the command cards) → tile.
    pub by_instance: HashMap<String, Rect>,
    /// `(is_you, slot)` → cell, occupied or not.
    pub by_slot: HashMap<(bool, Slot), Rect>,
}

impl BoardAnchors {
    /// The tile of a unit / captain, if it is on screen.
    pub fn instance(&self, id: &str) -> Option<Rect> {
        self.by_instance.get(id).copied()
    }

    /// The cell of a slot, if it is on screen.
    pub fn slot(&self, is_you: bool, slot: Slot) -> Option<Rect> {
        self.by_slot.get(&(is_you, slot)).copied()
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

/// The two header buttons.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum HeaderButton {
    EndTurn,
    Cancel,
}

/// A ring that breathes (attack / impact).
#[derive(Component, Clone, Copy)]
struct RingPulse {
    color: Color,
}

// ============================================================
// Skeleton
// ============================================================

#[allow(clippy::too_many_arguments)]
fn spawn_board(
    mut commands: Commands,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    session: Res<Session>,
    mode: Res<UiMode>,
    mut art: ResMut<ArtCache>,
    assets: Res<AssetServer>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cache: ResMut<BoardViewCache>,
    mut metrics: ResMut<BoardMetrics>,
) {
    let window_h = windows
        .single()
        .map(|w| w.resolution.height())
        .unwrap_or(l::WINDOW_H);
    metrics.0 = half_metrics(half_available_h(window_h));
    // The content is filled by the sync systems on the very next frame.
    cache.0 = None;

    let view = board_view(&session, &mode);
    let mut painter = Painter {
        commands: &mut commands,
        palette: &palette,
        fonts: &fonts,
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
    // under the cards.
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

fn spawn_header(painter: &mut Painter, root: Entity) {
    let palette = painter.palette;
    let bg = palette.bg_panel;
    let ink = palette.ink;
    let gold = palette.gold;
    let text_on_gold = palette.text_on_gold;
    let dim = palette.text_dim;
    let label = painter.fonts.oswald_bold.clone();

    let header = painter
        .commands
        .spawn((
            Node {
                width: percent(100.0),
                height: px(l::HEADER_H),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(8.0),
                padding: UiRect::horizontal(px(l::HEADER_PAD_X)),
                border: UiRect::bottom(px(2.0)),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor::all(ink),
            GlobalZIndex(l::Z_HEADER),
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
                column_gap: px(8.0),
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

    // "Fin de tour"
    let end_turn = painter
        .commands
        .spawn((
            Node {
                width: px(132.0),
                height: px(l::HEADER_H - 16.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(px(l::BUTTON_RADIUS)),
                margin: UiRect::left(px(10.0)),
                ..default()
            },
            BackgroundColor(gold),
            Button,
            HeaderButton::EndTurn,
            Name::new("EndTurnButton"),
            ChildOf(header),
        ))
        .id();
    painter
        .commands
        .spawn((
            style::text("Fin de tour", &label, l::FS_LABEL, text_on_gold),
            ChildOf(end_turn),
        ));
    painter.commands.entity(end_turn).observe(on_end_turn_clicked);

    // "Annuler"
    let cancel = painter
        .commands
        .spawn((
            Node {
                display: Display::None,
                width: px(84.0),
                height: px(l::HEADER_H - 16.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(px(1.0)),
                border_radius: BorderRadius::all(px(l::BUTTON_RADIUS)),
                margin: UiRect::left(px(6.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(ink),
            Button,
            HeaderButton::Cancel,
            Name::new("CancelButton"),
            ChildOf(header),
        ))
        .id();
    painter.commands.spawn((
        style::text("Annuler", &label, l::FS_LABEL, dim),
        ChildOf(cancel),
    ));
    painter.commands.entity(cancel).observe(on_cancel_clicked);
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
    painter.art(root, half.floor, Focus::CENTER, is_you);
    let shade = if is_you {
        LinearGradient::to_top(vec![
            ColorStop::new(Color::srgba(0.06, 0.16, 0.12, 0.55), percent(0.0)),
            ColorStop::new(Color::srgba(0.04, 0.05, 0.11, 0.40), percent(50.0)),
            ColorStop::new(half.ambiance, percent(100.0)),
        ])
    } else {
        LinearGradient::to_bottom(vec![
            ColorStop::new(Color::srgba(0.16, 0.06, 0.16, 0.55), percent(0.0)),
            ColorStop::new(Color::srgba(0.04, 0.05, 0.11, 0.40), percent(50.0)),
            ColorStop::new(half.ambiance, percent(100.0)),
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
                width: px(BATTLE_W),
                flex_direction: FlexDirection::Column,
                row_gap: px(metrics.gap),
                padding: UiRect::vertical(px(metrics.pad_y)),
                ..default()
            },
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
    let font = painter.fonts.oswald.clone();
    let row = painter
        .commands
        .spawn((
            Node {
                height: px(height),
                align_items: AlignItems::Center,
                padding: UiRect::left(px(3.0)),
                ..default()
            },
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
                column_gap: px(l::SLOT_GAP),
                ..default()
            },
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
    let content = painter.commands.spawn_empty().id();
    let veil = painter.commands.spawn_empty().id();

    let cell = painter
        .commands
        .spawn((
            Node {
                flex_grow: 1.0,
                flex_basis: px(0.0),
                height: percent(100.0),
                max_width: px(l::SLOT_W),
                overflow: Overflow::clip(),
                border: UiRect::all(px(2.0)),
                border_radius: BorderRadius::all(px(l::SLOT_RADIUS)),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor::all(ink),
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
            border_radius: BorderRadius::all(px(l::SLOT_RADIUS)),
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
    painter.commands.entity(cell).observe(on_cell_clicked);
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
                column_gap: px(l::CMD_GAP),
                ..default()
            },
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
                width: px(l::CAPTAIN_CARD_W),
                height: percent(100.0),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                border: UiRect::all(px(2.0)),
                border_radius: BorderRadius::all(px(12.0)),
                ..default()
            },
            BackgroundColor(slot_bg),
            BorderColor::all(side),
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
                width: px(l::SHIP_SLOT_W),
                height: percent(100.0),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                border: UiRect::all(px(1.0)),
                border_radius: BorderRadius::all(px(12.0)),
                ..default()
            },
            BackgroundColor(slot_bg),
            BorderColor::all(cyan.with_alpha(0.45)),
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
    mut cells: Query<(Entity, &CellNode, &mut CellCache)>,
    mut tints: Query<(&mut BorderColor, &mut BackgroundColor)>,
) {
    if !cache.is_changed() {
        return;
    }
    let Some(view) = cache.view() else { return };
    let palette = &*palette;
    let mut painter = Painter {
        commands: &mut commands,
        palette,
        fonts: &fonts,
        art: &mut art,
        assets: &assets,
        metrics: metrics.0,
    };

    for (entity, node, mut cached) in &mut cells {
        let Some(next) = view.half(node.is_you).cell(node.slot) else {
            continue;
        };
        let content_changed = cached.0.as_ref().is_none_or(|c| c.content != next.content);
        let decor_changed = cached.0.as_ref().is_none_or(|c| c.decor != next.decor);
        if content_changed {
            painter
                .commands
                .entity(node.content)
                .despawn_related::<Children>();
            painter.cell_content(node.content, next);
        }
        if content_changed || decor_changed {
            let resting = match &next.content {
                CellContent::Unit(unit) => unit.border,
                CellContent::Captain(_) => palette.foe,
                CellContent::Empty => palette.ink,
            };
            paint_decor(
                painter.commands,
                &mut tints,
                entity,
                node.veil,
                next.decor.ring,
                next.decor.dimmed,
                resting,
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
    mut captains: Query<(Entity, &CaptainNode, &mut CaptainCache)>,
    mut ships: Query<(&ShipNode, &mut ShipCache)>,
    mut resources: Query<(&ResourceNode, &mut ResourceCache)>,
    mut tints: Query<(&mut BorderColor, &mut BackgroundColor)>,
) {
    if !cache.is_changed() {
        return;
    }
    let Some(view) = cache.view() else { return };
    let palette = &*palette;
    let mut painter = Painter {
        commands: &mut commands,
        palette,
        fonts: &fonts,
        art: &mut art,
        assets: &assets,
        metrics: metrics.0,
    };

    for (entity, node, mut cached) in &mut captains {
        let next = &view.half(node.is_you).command.captain;
        if cached.0.as_ref() == Some(next) {
            continue;
        }
        let content_changed = cached
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
            entity,
            node.veil,
            next.decor.ring,
            next.decor.dimmed,
            palette.side(node.is_you),
            palette,
        );
        cached.0 = Some(next.clone());
    }

    for (node, mut cached) in &mut ships {
        let next = &view.half(node.is_you).command.ship;
        if cached.0.as_ref() == Some(next) {
            continue;
        }
        painter
            .commands
            .entity(node.content)
            .despawn_related::<Children>();
        painter.ship_content(node.content, next);
        cached.0 = Some(next.clone());
    }

    for (node, mut cached) in &mut resources {
        let next = view.half(node.is_you).command.resources;
        if cached.0 == Some(next) {
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
    mut art: ResMut<ArtCache>,
    assets: Res<AssetServer>,
    metrics: Res<BoardMetrics>,
    mut headers: Query<(&HeaderNode, &mut HeaderCache)>,
    mut buttons: Query<(&HeaderButton, &mut Node, &mut BackgroundColor)>,
) {
    if !cache.is_changed() {
        return;
    }
    let Some(view) = cache.view() else { return };
    let palette = &*palette;
    let mut painter = Painter {
        commands: &mut commands,
        palette,
        fonts: &fonts,
        art: &mut art,
        assets: &assets,
        metrics: metrics.0,
    };

    for (node, mut cached) in &mut headers {
        if cached.0.as_ref() == Some(&view.header) {
            continue;
        }
        painter
            .commands
            .entity(node.content)
            .despawn_related::<Children>();
        painter.header_content(node.content, &view.header);
        cached.0 = Some(view.header.clone());
    }

    for (kind, mut node, mut background) in &mut buttons {
        match kind {
            HeaderButton::EndTurn => {
                let color = if view.header.can_end_turn {
                    palette.gold
                } else {
                    palette.gold.with_alpha(0.28)
                };
                if background.0 != color {
                    background.0 = color;
                }
            }
            HeaderButton::Cancel => {
                let display = if view.header.can_cancel {
                    Display::Flex
                } else {
                    Display::None
                };
                if node.display != display {
                    node.display = display;
                }
            }
        }
    }
}

/// Re-tint a cell / captain card: ring, resting border and dim veil.
#[allow(clippy::too_many_arguments)]
fn paint_decor(
    commands: &mut Commands,
    tints: &mut Query<(&mut BorderColor, &mut BackgroundColor)>,
    entity: Entity,
    veil: Entity,
    ring: Ring,
    dimmed: bool,
    resting: Color,
    palette: &Palette,
) {
    let border = ring.color(palette).unwrap_or(resting);
    if let Ok((mut color, _)) = tints.get_mut(entity) {
        let next = BorderColor::all(border);
        if *color != next {
            *color = next;
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
fn pulse_rings(time: Res<Time>, mut rings: Query<(&RingPulse, &mut BorderColor)>) {
    let phase = (time.elapsed_secs() * 5.0).sin() * 0.5 + 0.5;
    let alpha = 0.45 + 0.55 * phase;
    for (pulse, mut border) in &mut rings {
        *border = BorderColor::all(pulse.color.with_alpha(alpha));
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
    cells: Query<(&CellNode, &CellCache, &ComputedNode, &UiGlobalTransform)>,
    captains: Query<(&CaptainNode, &ComputedNode, &UiGlobalTransform)>,
) {
    let anchors = anchors.bypass_change_detection();
    anchors.by_instance.clear();
    anchors.by_slot.clear();

    for (node, cached, computed, transform) in &cells {
        let rect = logical_rect(computed, transform);
        anchors.by_slot.insert((node.is_you, node.slot), rect);
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

fn on_cell_clicked(
    click: On<Pointer<Click>>,
    cells: Query<&CellNode>,
    session: Res<Session>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut dispatch: MessageWriter<DispatchAction>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
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
    session: Res<Session>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut dispatch: MessageWriter<DispatchAction>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
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
    session: Res<Session>,
    mut mode: ResMut<UiMode>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
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

fn on_end_turn_clicked(
    click: On<Pointer<Click>>,
    session: Res<Session>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut dispatch: MessageWriter<DispatchAction>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    if session.is_ai_turn() || session.in_counter_window() {
        return;
    }
    submit(
        UiCommand::Dispatch(GameAction::EndTurn),
        &mut mode,
        &mut selected,
        &mut dispatch,
    );
}

fn on_cancel_clicked(
    click: On<Pointer<Click>>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut dispatch: MessageWriter<DispatchAction>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    submit(UiCommand::Reset, &mut mode, &mut selected, &mut dispatch);
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
}

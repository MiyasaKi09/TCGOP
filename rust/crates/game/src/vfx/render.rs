//! The drawing half of [`crate::vfx`]: it consumes [`CombatVfx`] and the
//! [`RevealQueue`] and spawns UI entities that tween themselves out of
//! existence.
//!
//! Layers, bottom to top:
//!
//! | layer | z | content |
//! |---|---|---|
//! | ambient | `-1` / [`Z_COMMAND`](crate::app::layout::Z_COMMAND) | the drifting sea and the waterline shimmer |
//! | combat | [`Z_VFX`](crate::app::layout::Z_VFX) | projectiles, bursts, numbers, KO shards, slams |
//! | cut-in | `Z_MODAL + 5` | the special / captain cinematic |
//! | reveal | [`Z_REVEAL`](crate::app::layout::Z_REVEAL) | the centred play announcement |
//!
//! Positions come from [`BoardAnchors`](crate::board::BoardAnchors) — logical
//! pixels, origin at the top-left of the window — which is exactly the
//! coordinate space of an absolutely positioned node inside a full-screen root,
//! so no conversion is needed.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use core::time::Duration;

use crate::app::{AppScreen, AppSet, Fonts, Palette, layout as l};
use crate::art::{ArtCache, Focus, SEA_IMAGE, art_for, card_art_focus};
use crate::board::geometry::half_available_h;
use crate::board::widgets::CoverFit;
use crate::board::{BoardAnchors, BoardRoot};
use crate::bridge::Session;
use crate::selection::captain_key;
use crate::vfx::announce::{PlayAnnouncement, Side};
use crate::vfx::detect::{CombatEvent, Flash, VfxKind};
use crate::vfx::element::{HEAL_COLOR, HEAL_GLYPH, KO_GLYPH};
use crate::vfx::tween::{Tween, TweenState};
use crate::vfx::{
    CaptainFlipped, CombatVfx, ReducedMotion, RevealQueue, ScreenShake, SkipReveal, VfxSet,
    VfxSymbolFont, reset_vfx_state, vfx_ready,
};

// ============================================================
// Durations (CombatVfxLayer `DURATION`, CutInLayer `DUR`)
// ============================================================

const D_ATTACK: f32 = 0.560;
const D_IMPACT: f32 = 0.880;
const D_HEAL: f32 = 0.920;
const D_KO: f32 = 0.760;
const D_SPAWN: f32 = 0.500;
const D_TINT: f32 = 0.450;
const D_CUT_IN: f32 = 1.500;
const D_CUT_IN_SLIDE: f32 = 0.280;
const D_CUT_IN_FADE: f32 = 0.250;
const D_REVEAL_FLY: f32 = 0.500;
const D_REVEAL_FADE: f32 = 0.400;
const D_REVEAL_POP: f32 = 0.180;
const D_ENTRANCE: f32 = 0.550;

// ============================================================
// Components / resources
// ============================================================

/// Root of the combat layer.
#[derive(Component)]
pub struct VfxRoot;

/// Root of the play-reveal layer.
#[derive(Component)]
pub struct RevealRoot;

/// Root of one cut-in (there is at most one at a time).
#[derive(Component)]
pub struct CutIn;

/// A sea plate that scrolls horizontally and bobs vertically.
#[derive(Component, Debug, Clone, Copy)]
pub struct SeaScroll {
    /// Loops per second.
    pub speed: f32,
    /// Vertical bob, in logical pixels.
    pub bob: f32,
}

/// Opt-in hover animation: the node lifts and grows while the pointer is over
/// it. The hand rolls its own fan-aware version; this is for everything else.
#[derive(Component, Debug, Clone, Copy)]
pub struct HoverLift {
    /// Upward offset, in logical pixels.
    pub lift: f32,
    pub scale: f32,
    pub secs: f32,
}

impl Default for HoverLift {
    fn default() -> Self {
        HoverLift {
            lift: l::HAND_HOVER_LIFT,
            scale: 1.04,
            secs: 0.14,
        }
    }
}

/// The reveal currently on screen — TS `announcements[0]`.
#[derive(Resource, Debug, Default)]
pub struct ActiveReveal {
    pub entity: Option<Entity>,
    pub announcement: Option<PlayAnnouncement>,
    /// Seconds since the reveal appeared.
    pub elapsed: f32,
    /// Total lifetime, in seconds.
    pub total: f32,
    /// When the card starts flying / fading, in seconds.
    pub hold: f32,
    /// Has the fly-out already been armed?
    pub launched: bool,
}

impl ActiveReveal {
    fn clear(&mut self) {
        *self = ActiveReveal::default();
    }
}

/// Combat events whose tile is not on screen **yet**: the board publishes its
/// anchors from the computed UI layout, so a unit deployed this frame only gets
/// a rectangle on the next one. Such an event is retried for a few frames
/// instead of being dropped (a KO is the exception — its tile is on its way
/// out, so a missing anchor means "already gone").
#[derive(Resource, Debug, Default)]
pub struct PendingCombat(pub Vec<(CombatEvent, u8)>);

impl PendingCombat {
    /// How many frames an event waits for its anchor.
    pub const MAX_TRIES: u8 = 3;
}

/// Dismiss the cut-in currently on screen.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SkipCutIn;

// ============================================================
// Plugin
// ============================================================

/// Spawning, tweening and cleaning up every visual layer.
pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SkipCutIn>()
            .init_resource::<PendingCombat>()
            .add_systems(
                OnEnter(AppScreen::Board),
                spawn_layers.run_if(resource_exists::<AssetServer>),
            )
            .add_systems(OnExit(AppScreen::Board), (clear_reveal, reset_vfx_state))
            .add_systems(
                Update,
                (
                    render_combat_events,
                    play_cut_ins,
                    play_captain_flip,
                    drive_reveals,
                    apply_screen_shake,
                    animate_ambient,
                )
                    .in_set(AppSet::Vfx)
                    .after(VfxSet)
                    .run_if(vfx_ready)
                    .run_if(resource_exists::<AssetServer>),
            )
            .add_observer(hover_lift_enter)
            .add_observer(hover_lift_leave);
    }
}

// ============================================================
// Small helpers
// ============================================================

/// An absolutely positioned box of `size`, centred on `center`.
fn box_at(center: Vec2, size: Vec2) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(center.x - size.x * 0.5),
        top: px(center.y - size.y * 0.5),
        width: px(size.x),
        height: px(size.y),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..default()
    }
}

/// A full-screen, click-through overlay.
fn full_screen() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(0.0),
        top: px(0.0),
        width: percent(100.0),
        height: percent(100.0),
        ..default()
    }
}

fn label(value: impl Into<String>, font: &Handle<Font>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font: font.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        Pickable::IGNORE,
    )
}

fn secs(reduced: &ReducedMotion, value: f32) -> f32 {
    reduced.secs(value).max(0.05)
}

/// Where a unit / captain sits on screen.
fn center_of(anchors: &BoardAnchors, id: &str) -> Option<Vec2> {
    anchors.instance(id).map(|rect| rect.center())
}

/// The y of the waterline, from the window height alone (the terrain is laid
/// out by [`crate::board`] from the same numbers).
fn waterline_y(window_h: f32) -> f32 {
    l::HEADER_H + half_available_h(window_h) + l::WATERLINE_H * 0.5
}

fn window_size(windows: &Query<&Window, With<PrimaryWindow>>) -> Vec2 {
    windows
        .single()
        .map(|window| Vec2::new(window.resolution.width(), window.resolution.height()))
        .unwrap_or(Vec2::new(l::WINDOW_W, l::WINDOW_H))
}

// ============================================================
// Layers
// ============================================================

/// Spawn the ambient sea, the combat layer, the reveal layer and the entrance
/// stagger. Everything despawns with the board.
fn spawn_layers(
    mut commands: Commands,
    palette: Res<Palette>,
    mut art: ResMut<ArtCache>,
    assets: Res<AssetServer>,
    windows: Query<&Window, With<PrimaryWindow>>,
    reduced: Res<ReducedMotion>,
) {
    let size = window_size(&windows);
    let sea = art.image(&assets, SEA_IMAGE);

    // --- ambient: a wide, slowly drifting sea behind the terrain ---
    let ambient = commands
        .spawn((
            full_screen(),
            GlobalZIndex(-1),
            Pickable::IGNORE,
            DespawnOnExit(AppScreen::Board),
            Name::new("VfxAmbient"),
        ))
        .id();
    spawn_sea_plate(
        &mut commands,
        ambient,
        sea.clone(),
        SeaScroll {
            speed: 0.045,
            bob: 4.0,
        },
        palette.waterline.with_alpha(0.30),
    );

    // --- the waterline shimmer, a faster band right on the cut ---
    let band_h = l::WATERLINE_H * 3.0;
    let shimmer = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(waterline_y(size.y) - band_h * 0.5),
                width: percent(100.0),
                height: px(band_h),
                overflow: Overflow::clip(),
                ..default()
            },
            GlobalZIndex(l::Z_COMMAND),
            Pickable::IGNORE,
            DespawnOnExit(AppScreen::Board),
            Name::new("VfxWaterline"),
        ))
        .id();
    spawn_sea_plate(
        &mut commands,
        shimmer,
        sea,
        SeaScroll {
            speed: 0.14,
            bob: 2.0,
        },
        palette.gold.with_alpha(0.16),
    );

    // --- combat + reveal layers ---
    commands.spawn((
        full_screen(),
        GlobalZIndex(l::Z_VFX),
        Pickable::IGNORE,
        VfxRoot,
        DespawnOnExit(AppScreen::Board),
        Name::new("VfxCombat"),
    ));
    commands.spawn((
        full_screen(),
        GlobalZIndex(l::Z_REVEAL),
        Pickable::IGNORE,
        RevealRoot,
        DespawnOnExit(AppScreen::Board),
        Name::new("VfxReveals"),
    ));

    spawn_entrance(&mut commands, &palette, size, &reduced);
}

/// Two copies of the sea texture side by side (the second mirrored) inside a
/// double-width track — scrolling the track by half its width loops seamlessly.
fn spawn_sea_plate(
    commands: &mut Commands,
    parent: Entity,
    image: Handle<Image>,
    scroll: SeaScroll,
    tint: Color,
) {
    let track = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                width: percent(200.0),
                height: percent(100.0),
                flex_direction: FlexDirection::Row,
                ..default()
            },
            UiTransform::default(),
            scroll,
            Pickable::IGNORE,
            ChildOf(parent),
        ))
        .id();

    for mirrored in [false, true] {
        commands.spawn((
            Node {
                width: percent(50.0),
                height: percent(100.0),
                ..default()
            },
            ImageNode {
                image: image.clone(),
                image_mode: NodeImageMode::Stretch,
                flip_x: mirrored,
                color: tint,
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(track),
        ));
    }
}

/// The staggered arrival of the board: a full black-out that lifts, then three
/// bands (header, terrain, hand) that follow it a beat later.
fn spawn_entrance(
    commands: &mut Commands,
    palette: &Palette,
    size: Vec2,
    reduced: &ReducedMotion,
) {
    let duration = secs(reduced, D_ENTRANCE);
    commands.spawn((
        full_screen(),
        BackgroundColor(palette.bg_deep),
        GlobalZIndex(l::Z_REVEAL + 1),
        Pickable::IGNORE,
        DespawnOnExit(AppScreen::Board),
        Tween::secs(duration, TweenState::IDENTITY, TweenState::alpha(0.0))
            .with_ease(EaseFunction::QuadraticOut)
            .despawning(),
        Name::new("VfxEntrance"),
    ));

    let bands = [
        (0.0, l::HEADER_H, 0.0),
        (l::HEADER_H, (size.y - l::HEADER_H - l::HAND_H).max(0.0), 0.08),
        (size.y - l::HAND_H, l::HAND_H, 0.16),
    ];
    for (top, height, delay) in bands {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(top),
                width: percent(100.0),
                height: px(height),
                ..default()
            },
            BackgroundColor(palette.bg_deep.with_alpha(0.92)),
            GlobalZIndex(l::Z_REVEAL + 1),
            Pickable::IGNORE,
            DespawnOnExit(AppScreen::Board),
            UiTransform::default(),
            Tween::secs(
                secs(reduced, 0.40),
                TweenState::IDENTITY,
                TweenState::alpha(0.0),
            )
            .with_delay(Duration::from_secs_f32(secs(reduced, delay + 0.12)))
            .with_ease(EaseFunction::QuadraticOut)
            .despawning(),
            Name::new("VfxEntranceBand"),
        ));
    }
}

// ============================================================
// Combat layer
// ============================================================

/// Draw every [`CombatVfx`] of this frame (plus the ones still waiting for
/// their tile).
#[allow(clippy::too_many_arguments)]
fn render_combat_events(
    mut commands: Commands,
    mut incoming: MessageReader<CombatVfx>,
    mut pending: ResMut<PendingCombat>,
    roots: Query<Entity, With<VfxRoot>>,
    anchors: Res<BoardAnchors>,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    symbols: Res<VfxSymbolFont>,
    reduced: Res<ReducedMotion>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(root) = roots.single() else {
        incoming.clear();
        pending.0.clear();
        return;
    };
    let size = window_size(&windows);
    let queued: Vec<(CombatEvent, u8)> = std::mem::take(&mut pending.0);
    let mut retry = Vec::new();

    for (event, tries) in queued
        .into_iter()
        .chain(incoming.read().map(|CombatVfx(event)| (event.clone(), 0)))
    {
        let drawn = draw_event(
            &mut commands,
            root,
            &anchors,
            &palette,
            &fonts,
            &symbols,
            &reduced,
            size,
            &event,
        );
        if !drawn && event.kind != VfxKind::Ko && tries < PendingCombat::MAX_TRIES {
            retry.push((event, tries + 1));
        }
    }
    pending.0 = retry;
}

/// Draw one event; `false` when its tile has no anchor yet.
#[allow(clippy::too_many_arguments)]
fn draw_event(
    commands: &mut Commands,
    root: Entity,
    anchors: &BoardAnchors,
    palette: &Palette,
    fonts: &Fonts,
    symbols: &VfxSymbolFont,
    reduced: &ReducedMotion,
    size: Vec2,
    event: &CombatEvent,
) -> bool {
    let style = event.element.style();
    match event.kind {
        VfxKind::Attack => {
            let (Some(from), Some(to)) = (
                event.from_id.as_deref().and_then(|id| center_of(anchors, id)),
                event.to_id.as_deref().and_then(|id| center_of(anchors, id)),
            ) else {
                return false;
            };
            spawn_projectile(
                commands,
                root,
                symbols,
                reduced,
                event,
                from,
                to,
                style.color,
                style.glow,
            );
            if event.big && reduced.allows_screen_motion() {
                commands.spawn((
                    full_screen(),
                    BackgroundColor(style.tint),
                    Pickable::IGNORE,
                    Tween::secs(
                        secs(reduced, D_TINT),
                        TweenState::IDENTITY,
                        TweenState::alpha(0.0),
                    )
                    .despawning(),
                    ChildOf(root),
                ));
            }
            spawn_flash(commands, root, anchors, palette, reduced, event, size);
            true
        }
        VfxKind::Impact => {
            let Some(center) = event.to_id.as_deref().and_then(|id| center_of(anchors, id)) else {
                return false;
            };
            spawn_burst(
                commands,
                root,
                symbols,
                reduced,
                center,
                style.color,
                style.glyph,
                event.big,
                D_IMPACT,
            );
            if let Some(value) = event.value {
                spawn_number(
                    commands,
                    root,
                    fonts,
                    reduced,
                    center,
                    format!("-{value}"),
                    style.color,
                    event.big,
                    D_IMPACT,
                );
            }
            spawn_flash(commands, root, anchors, palette, reduced, event, size);
            true
        }
        VfxKind::Heal => {
            let Some(center) = event.to_id.as_deref().and_then(|id| center_of(anchors, id)) else {
                return false;
            };
            spawn_burst(
                commands,
                root,
                symbols,
                reduced,
                center,
                HEAL_COLOR,
                HEAL_GLYPH,
                false,
                D_HEAL,
            );
            if let Some(value) = event.value {
                spawn_number(
                    commands,
                    root,
                    fonts,
                    reduced,
                    center,
                    format!("+{value}"),
                    HEAL_COLOR,
                    false,
                    D_HEAL,
                );
            }
            spawn_flash(commands, root, anchors, palette, reduced, event, size);
            true
        }
        VfxKind::Ko => {
            let Some(center) = event.to_id.as_deref().and_then(|id| center_of(anchors, id)) else {
                return false;
            };
            spawn_ko(commands, root, symbols, palette, reduced, center);
            true
        }
        VfxKind::Spawn => {
            let Some(rect) = event.to_id.as_deref().and_then(|id| anchors.instance(id)) else {
                return false;
            };
            spawn_slam(commands, root, palette, reduced, rect);
            spawn_flash(commands, root, anchors, palette, reduced, event, size);
            true
        }
        // The attack name is the cut-in's job (the web layer also returns null
        // for `banner`).
        VfxKind::Banner => true,
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_projectile(
    commands: &mut Commands,
    root: Entity,
    symbols: &VfxSymbolFont,
    reduced: &ReducedMotion,
    event: &CombatEvent,
    from: Vec2,
    to: Vec2,
    color: Color,
    glow: Color,
) {
    let core = if event.big { 30.0 } else { 18.0 };
    let halo = core * 2.2;
    let duration = secs(reduced, D_ATTACK);
    let head = commands
        .spawn((
            Node {
                border_radius: BorderRadius::MAX,
                ..box_at(from, Vec2::splat(core))
            },
            BackgroundColor(color),
            UiTransform::default(),
            Pickable::IGNORE,
            Tween::secs(
                duration,
                TweenState::IDENTITY,
                TweenState::offset(to - from).with_uniform_scale(if event.zone { 1.8 } else { 0.9 }),
            )
            .with_ease(EaseFunction::QuadraticIn)
            .despawning(),
            ChildOf(root),
        ))
        .id();
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(-(halo - core) * 0.5),
            top: px(-(halo - core) * 0.5),
            width: px(halo),
            height: px(halo),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(glow.with_alpha(glow.alpha() * 0.45)),
        Pickable::IGNORE,
        ChildOf(head),
    ));
    commands.spawn((
        label(
            event.element.style().glyph,
            &symbols.0,
            if event.big { 20.0 } else { 13.0 },
            Color::WHITE,
        ),
        ChildOf(head),
    ));
}

#[allow(clippy::too_many_arguments)]
fn spawn_burst(
    commands: &mut Commands,
    root: Entity,
    symbols: &VfxSymbolFont,
    reduced: &ReducedMotion,
    center: Vec2,
    color: Color,
    glyph: &str,
    big: bool,
    duration: f32,
) {
    let size = if big { 78.0 } else { 48.0 };
    let burst = commands
        .spawn((
            Node {
                border: UiRect::all(px(if big { 4.0 } else { 3.0 })),
                border_radius: BorderRadius::MAX,
                ..box_at(center, Vec2::splat(size))
            },
            BorderColor::all(color),
            BackgroundColor(color.with_alpha(0.14)),
            UiTransform::default(),
            Pickable::IGNORE,
            Tween::secs(
                secs(reduced, duration),
                TweenState::scale(0.35),
                TweenState::scale(1.7).with_alpha(0.0),
            )
            .with_ease(EaseFunction::CubicOut)
            .despawning(),
            ChildOf(root),
        ))
        .id();
    commands.spawn((
        label(glyph, &symbols.0, if big { 26.0 } else { 18.0 }, color),
        ChildOf(burst),
    ));
}

/// A Bangers damage / healing number rising from the tile.
#[allow(clippy::too_many_arguments)]
fn spawn_number(
    commands: &mut Commands,
    root: Entity,
    fonts: &Fonts,
    reduced: &ReducedMotion,
    center: Vec2,
    value: String,
    color: Color,
    big: bool,
    duration: f32,
) {
    let size = if big { l::FS_DAMAGE } else { l::FS_DAMAGE * 0.62 };
    let node = commands
        .spawn((
            box_at(center, Vec2::new(160.0, size * 1.4)),
            UiTransform::default(),
            Pickable::IGNORE,
            Tween::secs(
                secs(reduced, duration),
                TweenState::scale(0.7),
                TweenState::offset(Vec2::new(0.0, -38.0))
                    .with_uniform_scale(1.0)
                    .with_alpha(0.0),
            )
            .with_ease(EaseFunction::CubicOut)
            .despawning(),
            ChildOf(root),
        ))
        .id();
    commands.spawn((
        label(value, &fonts.bangers, size, color),
        TextShadow {
            offset: Vec2::splat(2.0),
            color: Color::BLACK.with_alpha(0.8),
        },
        ChildOf(node),
    ));
}

/// The tile flash — the DOM classes `vfx-hit` / `vfx-knockback` / `vfx-heal` /
/// `vfx-slam` / `vfx-lunge`, drawn as an overlay over the tile so the board's
/// own entities are never touched.
fn spawn_flash(
    commands: &mut Commands,
    root: Entity,
    anchors: &BoardAnchors,
    palette: &Palette,
    reduced: &ReducedMotion,
    event: &CombatEvent,
    window: Vec2,
) {
    let Some(flash) = event.flash else {
        return;
    };
    let Some(rect) = event.flash_target().and_then(|id| anchors.instance(id)) else {
        return;
    };
    let duration = secs(reduced, flash.millis() as f32 / 1000.0);
    let center = rect.center();
    // Push away from the waterline: the foe's half is above it, yours below.
    let away = if center.y < window.y * 0.5 { -1.0 } else { 1.0 };
    let (color, offset) = match flash {
        Flash::Lunge => (palette.gold.with_alpha(0.22), Vec2::new(0.0, -6.0 * away)),
        Flash::Hit => (palette.atk.with_alpha(0.34), Vec2::ZERO),
        Flash::Knockback => (palette.impact.with_alpha(0.40), Vec2::new(0.0, 10.0 * away)),
        Flash::Heal => (palette.hp.with_alpha(0.28), Vec2::ZERO),
        Flash::Slam => (Color::WHITE.with_alpha(0.35), Vec2::ZERO),
    };
    commands.spawn((
        Node {
            border_radius: BorderRadius::all(px(l::SLOT_RADIUS)),
            ..box_at(center, rect.size())
        },
        BackgroundColor(color),
        UiTransform::default(),
        Pickable::IGNORE,
        Tween::secs(
            duration,
            TweenState::IDENTITY,
            TweenState::offset(offset).with_alpha(0.0),
        )
        .with_ease(EaseFunction::QuadraticOut)
        .despawning(),
        ChildOf(root),
    ));
}

/// KO: a handful of shards flying apart behind a big `✖`.
fn spawn_ko(
    commands: &mut Commands,
    root: Entity,
    symbols: &VfxSymbolFont,
    palette: &Palette,
    reduced: &ReducedMotion,
    center: Vec2,
) {
    let duration = secs(reduced, D_KO);
    for index in 0..6u32 {
        let angle = index as f32 / 6.0 * core::f32::consts::TAU;
        let direction = Vec2::new(ops::cos(angle), ops::sin(angle));
        commands.spawn((
            Node {
                border_radius: BorderRadius::all(px(2.0)),
                ..box_at(center, Vec2::new(14.0, 6.0))
            },
            BackgroundColor(palette.text.with_alpha(0.85)),
            UiTransform::default(),
            Pickable::IGNORE,
            Tween::secs(
                duration,
                TweenState::IDENTITY.with_rotation(angle),
                TweenState::offset(direction * 52.0)
                    .with_rotation(angle + 1.2)
                    .with_alpha(0.0),
            )
            .with_ease(EaseFunction::CubicOut)
            .despawning(),
            ChildOf(root),
        ));
    }
    let cross = commands
        .spawn((
            box_at(center, Vec2::splat(60.0)),
            UiTransform::default(),
            Pickable::IGNORE,
            Tween::secs(
                duration,
                TweenState::scale(0.6),
                TweenState::scale(1.6).with_alpha(0.0),
            )
            .with_ease(EaseFunction::CubicOut)
            .despawning(),
            ChildOf(root),
        ))
        .id();
    commands.spawn((
        label(KO_GLYPH, &symbols.0, 40.0, palette.atk),
        ChildOf(cross),
    ));
}

/// Deploy: a gold ring that snaps down onto the slot.
fn spawn_slam(
    commands: &mut Commands,
    root: Entity,
    palette: &Palette,
    reduced: &ReducedMotion,
    rect: Rect,
) {
    commands.spawn((
        Node {
            border: UiRect::all(px(2.0)),
            border_radius: BorderRadius::all(px(l::SLOT_RADIUS)),
            ..box_at(rect.center(), rect.size())
        },
        BorderColor::all(palette.gold),
        BackgroundColor(palette.gold.with_alpha(0.12)),
        UiTransform::default(),
        Pickable::IGNORE,
        Tween::secs(
            secs(reduced, D_SPAWN),
            TweenState::scale(1.28),
            TweenState::scale(1.0).with_alpha(0.0),
        )
        .with_ease(EaseFunction::CubicOut)
        .despawning(),
        ChildOf(root),
    ));
}

// ============================================================
// Cut-in
// ============================================================

/// The signature cinematic of a special / captain attack — `vfx/CutInLayer.tsx`.
#[allow(clippy::too_many_arguments)]
fn play_cut_ins(
    mut commands: Commands,
    mut incoming: MessageReader<CombatVfx>,
    mut skips: MessageReader<SkipCutIn>,
    existing: Query<Entity, With<CutIn>>,
    anchors: Res<BoardAnchors>,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    mut art: ResMut<ArtCache>,
    assets: Res<AssetServer>,
    reduced: Res<ReducedMotion>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let skipped = skips.read().count() > 0;
    let wanted = incoming
        .read()
        .map(|CombatVfx(event)| event)
        .filter(|event| event.wants_cut_in())
        .last()
        .cloned();

    if skipped || wanted.is_some() {
        for entity in &existing {
            commands.entity(entity).try_despawn();
        }
    }
    let Some(event) = wanted else {
        return;
    };

    let size = window_size(&windows);
    let style = event.element.style();
    let from_left = event
        .from_id
        .as_deref()
        .and_then(|id| center_of(&anchors, id))
        .map(|center| center.x < size.x * 0.5)
        .unwrap_or(true);
    let duration = if reduced.allows_screen_motion() {
        D_CUT_IN
    } else {
        0.450
    };
    let band_h = 148.0;

    let root = commands
        .spawn((
            full_screen(),
            GlobalZIndex(l::Z_MODAL + 5),
            CutIn,
            DespawnOnExit(AppScreen::Board),
            // Hold, then fade out over the last quarter of a second.
            Tween::secs(D_CUT_IN_FADE, TweenState::IDENTITY, TweenState::alpha(0.0))
                .with_delay(Duration::from_secs_f32(
                    (duration - D_CUT_IN_FADE).max(0.05),
                ))
                .with_ease(EaseFunction::QuadraticIn)
                .despawning(),
            Name::new("VfxCutIn"),
        ))
        .observe(
            |_click: On<Pointer<Click>>, mut skip: MessageWriter<SkipCutIn>| {
                skip.write(SkipCutIn);
            },
        )
        .id();

    // Vignette (also the click catcher).
    commands.spawn((
        full_screen(),
        BackgroundColor(palette.scrim.with_alpha(0.55)),
        ChildOf(root),
    ));

    let band = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(size.y * 0.42 - band_h * 0.5),
                width: percent(100.0),
                height: px(band_h),
                flex_direction: if from_left {
                    FlexDirection::Row
                } else {
                    FlexDirection::RowReverse
                },
                align_items: AlignItems::Center,
                column_gap: px(18.0),
                padding: UiRect::horizontal(px(28.0)),
                border: UiRect::vertical(px(2.0)),
                ..default()
            },
            BackgroundColor(palette.bg_deep.with_alpha(0.88)),
            BorderColor::all(style.color.with_alpha(0.8)),
            UiTransform::default(),
            Pickable::IGNORE,
            // Reduced motion keeps the fade but drops the slide.
            Tween::secs(
                secs(&reduced, D_CUT_IN_SLIDE),
                TweenState::offset(if reduced.allows_screen_motion() {
                    Vec2::new(
                        if from_left { -size.x * 0.35 } else { size.x * 0.35 },
                        0.0,
                    )
                } else {
                    Vec2::ZERO
                })
                .with_alpha(0.0),
                TweenState::IDENTITY,
            )
            .with_ease(EaseFunction::CubicOut),
            ChildOf(root),
        ))
        .id();

    // The attacker's art panel (verso first, like the web cut-in).
    if let Some(path) = event
        .def_id
        .as_deref()
        .and_then(|def_id| art_for(def_id, true))
    {
        let panel = commands
            .spawn((
                Node {
                    width: px(196.0),
                    height: px(band_h - 24.0),
                    overflow: Overflow::clip(),
                    border: UiRect::all(px(2.0)),
                    border_radius: BorderRadius::all(px(10.0)),
                    ..default()
                },
                BorderColor::all(style.color),
                BackgroundColor(palette.bg_panel),
                Pickable::IGNORE,
                ChildOf(band),
            ))
            .id();
        let focus = match event.def_id.as_deref().map(card_art_focus) {
            Some(focus) if focus != Focus::CENTER => focus,
            _ => Focus::new(0.5, l::CAPTAIN_ART_FOCUS_Y),
        };
        let handle = art.image(&assets, path);
        commands.spawn((
            ImageNode {
                image: handle,
                image_mode: NodeImageMode::Stretch,
                ..default()
            },
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                width: percent(100.0),
                height: percent(100.0),
                ..default()
            },
            CoverFit { focus },
            Pickable::IGNORE,
            ChildOf(panel),
        ));
    }

    let text_column = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4.0),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(band),
        ))
        .id();
    if let Some(name) = event.attack_name.as_deref() {
        commands.spawn((
            label(name, &fonts.cinzel_bold, 24.0, style.color),
            ChildOf(text_column),
        ));
    }
    if let Some(name) = event.attacker_name.as_deref() {
        commands.spawn((
            label(name, &fonts.cinzel, 15.0, palette.text),
            ChildOf(text_column),
        ));
    }
}

// ============================================================
// Captain flip
// ============================================================

/// A 3D-ish flip: the recto squashes to nothing on the X axis, the verso opens
/// back up behind a gold flash.
#[allow(clippy::too_many_arguments)]
fn play_captain_flip(
    mut commands: Commands,
    mut flips: MessageReader<CaptainFlipped>,
    roots: Query<Entity, With<VfxRoot>>,
    anchors: Res<BoardAnchors>,
    session: Res<Session>,
    palette: Res<Palette>,
    mut art: ResMut<ArtCache>,
    assets: Res<AssetServer>,
    reduced: Res<ReducedMotion>,
) {
    let Ok(root) = roots.single() else {
        flips.clear();
        return;
    };
    for CaptainFlipped(player) in flips.read().copied() {
        let key = captain_key(player);
        let Some(rect) = anchors.instance(&key) else {
            continue;
        };
        let def_id = session.state.player(player).captain.def_id.clone();
        let half = secs(&reduced, 0.22);

        for (flipped, from, to, delay) in [
            (
                false,
                TweenState::IDENTITY,
                TweenState::IDENTITY.with_scale(Vec2::new(0.0, 1.0)),
                0.0,
            ),
            (
                true,
                TweenState::IDENTITY.with_scale(Vec2::new(0.0, 1.0)),
                TweenState::IDENTITY,
                half,
            ),
        ] {
            let Some(path) = art_for(&def_id, flipped) else {
                continue;
            };
            let handle = art.image(&assets, path);
            let face = commands
                .spawn((
                    Node {
                        overflow: Overflow::clip(),
                        border_radius: BorderRadius::all(px(10.0)),
                        ..box_at(rect.center(), rect.size())
                    },
                    BackgroundColor(palette.bg_panel),
                    UiTransform::default(),
                    Pickable::IGNORE,
                    Tween::secs(half, from, to)
                        .with_delay(Duration::from_secs_f32(delay))
                        .with_ease(EaseFunction::QuadraticInOut)
                        .despawning(),
                    ChildOf(root),
                ))
                .id();
            commands.spawn((
                ImageNode {
                    image: handle,
                    image_mode: NodeImageMode::Stretch,
                    ..default()
                },
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    top: px(0.0),
                    width: percent(100.0),
                    height: percent(100.0),
                    ..default()
                },
                CoverFit {
                    focus: Focus::new(0.5, l::CAPTAIN_ART_FOCUS_Y),
                },
                Pickable::IGNORE,
                ChildOf(face),
            ));
        }

        commands.spawn((
            Node {
                border_radius: BorderRadius::all(px(10.0)),
                ..box_at(rect.center(), rect.size())
            },
            BackgroundColor(palette.gold.with_alpha(0.45)),
            UiTransform::default(),
            Pickable::IGNORE,
            Tween::secs(
                secs(&reduced, 0.55),
                TweenState::IDENTITY,
                TweenState::scale(1.15).with_alpha(0.0),
            )
            .with_ease(EaseFunction::CubicOut)
            .despawning(),
            ChildOf(root),
        ));
    }
}

// ============================================================
// Play reveals
// ============================================================

/// Pop the head of the [`RevealQueue`], hold it, then fly it to its tile —
/// `PlayRevealLayer.tsx`.
#[allow(clippy::too_many_arguments)]
fn drive_reveals(
    time: Res<Time>,
    mut commands: Commands,
    mut active: ResMut<ActiveReveal>,
    mut queue: ResMut<RevealQueue>,
    mut skips: MessageReader<SkipReveal>,
    roots: Query<Entity, With<RevealRoot>>,
    anchors: Res<BoardAnchors>,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    mut art: ResMut<ArtCache>,
    assets: Res<AssetServer>,
    reduced: Res<ReducedMotion>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let skipped = skips.read().count() > 0;
    let Ok(root) = roots.single() else {
        return;
    };
    let size = window_size(&windows);

    if let Some(entity) = active.entity {
        active.elapsed += time.delta_secs();
        let done = skipped || active.elapsed >= active.total;
        if done {
            commands.entity(entity).try_despawn();
            active.clear();
        } else if !active.launched && active.elapsed >= active.hold {
            active.launched = true;
            let announcement = active.announcement.clone();
            let card = card_size(announcement.as_ref());
            let dest = announcement
                .as_ref()
                .filter(|announcement| !announcement.toast)
                .and_then(|announcement| announcement.dest_id.as_deref())
                .and_then(|id| center_of(&anchors, id));
            let tween = match dest {
                Some(dest) => Tween::secs(
                    secs(&reduced, D_REVEAL_FLY),
                    TweenState::IDENTITY,
                    TweenState::offset(dest - reveal_center(size, card))
                        .with_uniform_scale(0.16)
                        .with_alpha(0.0),
                )
                .with_ease(EaseFunction::CubicInOut),
                None => Tween::secs(
                    secs(&reduced, D_REVEAL_FADE),
                    TweenState::IDENTITY,
                    TweenState::alpha(0.0),
                ),
            };
            commands.entity(entity).try_insert(tween.keeping());
        }
        return;
    }

    let Some(announcement) = queue.pop() else {
        return;
    };
    let total = reduced.duration(announcement.duration()).as_secs_f32();
    let entity = spawn_reveal(
        &mut commands,
        root,
        &palette,
        &fonts,
        &mut art,
        &assets,
        &reduced,
        &announcement,
        size,
    );
    *active = ActiveReveal {
        entity: Some(entity),
        hold: total * announcement.hold_fraction(),
        total,
        announcement: Some(announcement),
        elapsed: 0.0,
        launched: false,
    };
}

/// Size of the reveal card — TS `FullCard width={ann.big ? 300 : 184}`, scaled
/// to this window.
fn card_size(announcement: Option<&PlayAnnouncement>) -> Vec2 {
    match announcement {
        Some(announcement) if announcement.toast || announcement.def_id.is_none() => {
            Vec2::new(320.0, 74.0)
        }
        Some(announcement) if announcement.big => {
            Vec2::new(l::FULL_CARD_W, l::FULL_CARD_H + 66.0)
        }
        _ => Vec2::new(170.0, 170.0 * l::CARD_ASPECT + 60.0),
    }
}

fn reveal_center(window: Vec2, _card: Vec2) -> Vec2 {
    Vec2::new(window.x * 0.5, window.y * 0.42)
}

#[allow(clippy::too_many_arguments)]
fn spawn_reveal(
    commands: &mut Commands,
    root: Entity,
    palette: &Palette,
    fonts: &Fonts,
    art: &mut ArtCache,
    assets: &AssetServer,
    reduced: &ReducedMotion,
    announcement: &PlayAnnouncement,
    window: Vec2,
) -> Entity {
    let accent = match announcement.side {
        Side::You => palette.deploy,
        Side::Foe => palette.target,
    };
    let size = card_size(Some(announcement));
    let center = reveal_center(window, size);

    let card = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(6.0),
                padding: UiRect::all(px(10.0)),
                border: UiRect::all(px(2.0)),
                border_radius: BorderRadius::all(px(l::PANEL_RADIUS)),
                ..box_at(center, size)
            },
            BackgroundColor(palette.bg_panel),
            BorderColor::all(accent),
            UiTransform::default(),
            Tween::secs(
                secs(reduced, D_REVEAL_POP),
                TweenState::scale(0.72).with_alpha(0.0),
                TweenState::IDENTITY,
            )
            .with_ease(EaseFunction::CubicOut)
            .keeping(),
            ChildOf(root),
        ))
        .observe(
            |_click: On<Pointer<Click>>, mut skip: MessageWriter<SkipReveal>| {
                skip.write(SkipReveal);
            },
        )
        .id();

    commands.spawn((
        label(
            announcement.side.label(),
            &fonts.oswald_bold,
            l::FS_LABEL,
            accent,
        ),
        ChildOf(card),
    ));

    if !announcement.toast
        && let Some(path) = announcement
            .def_id
            .as_deref()
            .and_then(|def_id| art_for(def_id, false))
        {
            let art_w = size.x - 20.0;
            let frame = commands
                .spawn((
                    Node {
                        width: px(art_w),
                        height: px(art_w * l::CARD_ASPECT * 0.72),
                        overflow: Overflow::clip(),
                        border_radius: BorderRadius::all(px(10.0)),
                        ..default()
                    },
                    BackgroundColor(palette.bg_slot),
                    Pickable::IGNORE,
                    ChildOf(card),
                ))
                .id();
            let focus = announcement
                .def_id
                .as_deref()
                .map(card_art_focus)
                .unwrap_or(Focus::CENTER);
            let handle = art.image(assets, path);
            commands.spawn((
                ImageNode {
                    image: handle,
                    image_mode: NodeImageMode::Stretch,
                    ..default()
                },
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    top: px(0.0),
                    width: percent(100.0),
                    height: percent(100.0),
                    ..default()
                },
                CoverFit { focus },
                Pickable::IGNORE,
                ChildOf(frame),
            ));
        }

    commands.spawn((
        Text::new(announcement.caption.clone()),
        TextFont {
            font: fonts.oswald.clone().into(),
            font_size: FontSize::Px(l::FS_BODY),
            ..default()
        },
        TextColor(palette.text),
        TextLayout::new(Justify::Center, LineBreak::WordBoundary),
        Node {
            max_width: px(size.x - 20.0),
            ..default()
        },
        Pickable::IGNORE,
        ChildOf(card),
    ));

    card
}

/// Drop whatever reveal was on screen (leaving the board, or a new game).
fn clear_reveal(mut commands: Commands, mut active: ResMut<ActiveReveal>) {
    if let Some(entity) = active.entity {
        commands.entity(entity).try_despawn();
    }
    active.clear();
}

// ============================================================
// Screen shake / ambient / hover
// ============================================================

/// Offset the whole board while a shake is running.
fn apply_screen_shake(
    time: Res<Time>,
    mut shake: ResMut<ScreenShake>,
    mut boards: Query<&mut UiTransform, With<BoardRoot>>,
) {
    let offset = shake.advance(time.delta_secs());
    let target = Val2::px(offset.x, offset.y);
    for mut transform in &mut boards {
        if transform.translation != target {
            transform.translation = target;
        }
    }
}

/// Scroll and bob the sea plates.
fn animate_ambient(
    time: Res<Time>,
    reduced: Res<ReducedMotion>,
    mut plates: Query<(&SeaScroll, &mut UiTransform)>,
) {
    if !reduced.allows_screen_motion() {
        return;
    }
    let elapsed = time.elapsed_secs();
    for (scroll, mut transform) in &mut plates {
        let phase = (elapsed * scroll.speed).fract();
        transform.translation = Val2::new(
            Val::Percent(-phase * 50.0),
            Val::Px(ops::sin(elapsed * 0.9) * scroll.bob),
        );
    }
}

fn hover_state(transform: Option<&UiTransform>) -> TweenState {
    match transform {
        Some(transform) => TweenState {
            offset: Vec2::new(
                if let Val::Px(value) = transform.translation.x { value } else { 0.0 },
                if let Val::Px(value) = transform.translation.y { value } else { 0.0 },
            ),
            scale: transform.scale,
            rotation: 0.0,
            alpha: 1.0,
        },
        None => TweenState::IDENTITY,
    }
}

/// Lift a [`HoverLift`] node when the pointer enters it.
fn hover_lift_enter(
    over: On<Pointer<Enter>>,
    lifts: Query<(&HoverLift, Option<&UiTransform>)>,
    reduced: Res<ReducedMotion>,
    mut commands: Commands,
) {
    let Ok((lift, transform)) = lifts.get(over.entity) else {
        return;
    };
    let from = hover_state(transform);
    let to = TweenState::offset(Vec2::new(0.0, -lift.lift)).with_uniform_scale(lift.scale);
    commands.entity(over.entity).try_insert((
        UiTransform::default(),
        Tween::secs(secs(&reduced, lift.secs), from, to)
            .with_ease(EaseFunction::CubicOut)
            .keeping(),
    ));
}

/// Put it back down.
fn hover_lift_leave(
    leave: On<Pointer<Leave>>,
    lifts: Query<(&HoverLift, Option<&UiTransform>)>,
    reduced: Res<ReducedMotion>,
    mut commands: Commands,
) {
    let Ok((lift, transform)) = lifts.get(leave.entity) else {
        return;
    };
    let from = hover_state(transform);
    commands.entity(leave.entity).try_insert(
        Tween::secs(secs(&reduced, lift.secs), from, TweenState::IDENTITY)
            .with_ease(EaseFunction::CubicOut)
            .keeping(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_waterline_sits_between_the_two_halves() {
        let y = waterline_y(l::WINDOW_H);
        assert!(y > l::HEADER_H);
        assert!(y < l::WINDOW_H - l::HAND_H);
        // Symmetry: the two halves are the same height around it.
        let top = y - l::HEADER_H - l::WATERLINE_H * 0.5;
        let bottom = (l::WINDOW_H - l::HAND_H) - (y + l::WATERLINE_H * 0.5);
        assert!((top - bottom).abs() < 0.01, "{top} vs {bottom}");
    }

    #[test]
    fn a_box_is_centred_on_its_anchor() {
        let node = box_at(Vec2::new(100.0, 80.0), Vec2::new(40.0, 20.0));
        assert_eq!(node.left, px(80.0));
        assert_eq!(node.top, px(70.0));
        assert_eq!(node.width, px(40.0));
    }

    #[test]
    fn a_toast_is_smaller_than_a_full_card() {
        let toast = PlayAnnouncement {
            side: Side::You,
            def_id: None,
            instance_id: None,
            kind: "endTurn",
            dest_id: None,
            caption: "Fin de tour".into(),
            big: false,
            toast: true,
        };
        let mut big = toast.clone();
        big.toast = false;
        big.big = true;
        big.def_id = Some("MG-001".into());
        assert!(card_size(Some(&toast)).y < card_size(Some(&big)).y);
    }

    #[test]
    fn the_reveal_is_centred_high_on_the_screen() {
        let window = Vec2::new(l::WINDOW_W, l::WINDOW_H);
        let center = reveal_center(window, Vec2::splat(200.0));
        assert_eq!(center.x, window.x * 0.5);
        assert!(center.y < window.y * 0.5, "above the hand");
    }
}

//! Every modal of the client, as Bevy UI overlays.
//!
//! One system rebuilds the whole overlay stack whenever the
//! [`UiMode`](crate::selection::UiMode) or the engine state changes:
//!
//! | mode / condition | web | built by |
//! |---|---|---|
//! | `ActionMenu` | `ActionMenu.tsx` + the mock-up's `.pop` | [`spawn_action_menu`] |
//! | `CaptainMenu` | `CaptainMenu.tsx` | [`spawn_captain_menu`] |
//! | `ShipMenu` | `ShipMenu.tsx` | [`spawn_ship_menu`] |
//! | `CardDetail` | `CardDetail.tsx` + `FullCard.tsx` | [`spawn_card_detail`] |
//! | `ConfirmEvent` / `ConfirmShip` | `EventConfirm.tsx` | [`spawn_confirm`] |
//! | `Session::in_counter_window` | `Game.tsx::renderCounterWindow` | [`spawn_counter_window`] |
//!
//! The counter window is **not** a [`UiMode`](crate::selection::UiMode) arm: it
//! is driven by [`Session::in_counter_window`](crate::bridge::Session::in_counter_window)
//! and is drawn above any open modal, exactly like the TSX (`z-50` over `z-40`).
//!
//! Every panel is a scrim + a gold-on-navy body. Clicking the scrim closes back
//! to `Idle`; clicking the body is swallowed (TS `e.stopPropagation()`); every
//! button carries a [`ClickCommand`](widgets::ClickCommand) that the single
//! global observer [`route_panel_clicks`] turns into `resetUI()` plus a
//! [`DispatchAction`](crate::bridge::DispatchAction).
//!
//! The **action menu is the exception**: the mock-up's core interaction is a
//! small popover anchored on the tile you clicked ("Carte cliquée = infos +
//! actions · sinon pleine illustration"), so it has no dimmer and no centred
//! body — the board has to stay visible around it. It is placed from
//! [`BoardAnchors`](crate::board::BoardAnchors) and re-placed every frame by
//! [`place_popovers`], and a transparent full-screen catcher takes the click
//! that closes it.
//!
//! **Rebuilds.** An open panel is respawned only when its own content changed:
//! [`panel_signature`] fingerprints exactly what each panel draws, so a card
//! detail left open while the AI plays no longer flickers once per AI action.
//!
//! The amber "clique ton Capitaine / ton Navire" hint box lives in the action
//! column next to the hand and is built by
//! [`hand`](crate::hand) (`footer_hints`), not here.

pub mod model;
pub mod widgets;

use bevy::input::mouse::MouseScrollUnit;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use tcgop_engine::state::CardInstance;
use tcgop_engine::types::{CardDef, PlayerId};

use crate::app::{AppScreen, AppSet, Fonts, Palette, configure_pipeline, layout as L};
use crate::art::{ArtCache, faction_visual, trait_color, trait_label};
use crate::board::BoardAnchors;
use crate::board::interaction::apply_ui_command;
use crate::bridge::{BridgeSet, DispatchAction, Session, StateChanged};
use crate::hand::SymbolFont;
use crate::hand::card_face::{CardFace, FaceCtx, element_color, element_label, spawn_card_face};
use crate::selection::{SelectedHandCard, UiCommand, UiMode};

use model::{
    AbilityKind, AbilityRow, AttackOption, CounterKind, StatusChip, haki_label, turns_label,
};
use widgets::{
    ANCHOR, BOLT, ButtonSpec, ButtonTone, CROWN, ClickCommand, EYE, HEART, PanelButton, PanelCtx,
    SHIELD, SPARKLE, STAR, SWORD, body, button_gradient, catcher, column, line, panel, popover,
    row, scrim, section, spawn_button, spawn_caption, spawn_close_cross, spawn_gauge, spawn_pill,
};

// ============================================================
// Plugin
// ============================================================

/// All modals: action menu, captain menu, ship menu, card detail,
/// confirmations and the counter window.
pub struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        configure_pipeline(app);
        app
            // `HandPlugin` inserts this first; the fallback keeps `PanelsPlugin`
            // usable on its own in a head-less test.
            .init_resource::<SymbolFont>()
            // `BoardPlugin` owns it; the fallback keeps the popover placement
            // working when the panels are assembled on their own.
            .init_resource::<BoardAnchors>()
            .init_resource::<PanelCache>()
            .add_observer(route_panel_clicks)
            .add_systems(
                Update,
                (rebuild_panels, place_popovers, highlight_panel_buttons)
                    .chain()
                    .in_set(AppSet::Render)
                    .after(BridgeSet)
                    .run_if(in_state(AppScreen::Board))
                    .run_if(resource_exists::<Session>)
                    .run_if(resource_exists::<AssetServer>),
            )
            .add_systems(OnExit(AppScreen::Board), close_all_panels);
    }
}

// ============================================================
// Which panels are up
// ============================================================

/// One overlay to draw, in painting order (last one is on top).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelKind {
    ActionMenu {
        instance_id: String,
    },
    CaptainMenu {
        player: PlayerId,
    },
    ShipMenu {
        instance_id: String,
        is_you: bool,
    },
    CardDetail {
        def_id: String,
        instance_id: Option<String>,
    },
    Confirm {
        instance_id: String,
        is_ship: bool,
    },
    Counter,
}

/// Which overlays the current [`UiMode`] and counter state ask for.
///
/// TS renders `renderCounterWindow()` *and* the `uiMode` modal, so both can be
/// on screen at once; the counter window is appended last and therefore wins.
pub fn active_panels(mode: &UiMode, in_counter_window: bool) -> Vec<PanelKind> {
    let mut panels = Vec::new();
    match mode {
        UiMode::ActionMenu { instance_id } => panels.push(PanelKind::ActionMenu {
            instance_id: instance_id.clone(),
        }),
        UiMode::CaptainMenu { player_id } => {
            panels.push(PanelKind::CaptainMenu { player: *player_id })
        }
        UiMode::ShipMenu {
            instance_id,
            is_you,
        } => panels.push(PanelKind::ShipMenu {
            instance_id: instance_id.clone(),
            is_you: *is_you,
        }),
        UiMode::CardDetail {
            def_id,
            instance_id,
        } => panels.push(PanelKind::CardDetail {
            def_id: def_id.clone(),
            instance_id: instance_id.clone(),
        }),
        UiMode::ConfirmEvent { instance_id } => panels.push(PanelKind::Confirm {
            instance_id: instance_id.clone(),
            is_ship: false,
        }),
        UiMode::ConfirmShip { instance_id } => panels.push(PanelKind::Confirm {
            instance_id: instance_id.clone(),
            is_ship: true,
        }),
        _ => {}
    }
    if in_counter_window {
        panels.push(PanelKind::Counter);
    }
    panels
}

// ============================================================
// Rebuild
// ============================================================

/// Root of one overlay (scrim + panel). Despawned wholesale on every rebuild.
#[derive(Component)]
struct PanelRoot;

/// What the overlay stack was last built from.
#[derive(Resource, Default)]
struct PanelCache {
    key: Option<Vec<PanelKind>>,
    /// What each open panel last drew, as a comparable fingerprint.
    ///
    /// The layer used to bump a stamp on **every** `StateChanged` and treat a
    /// stamp mismatch as a full rebuild, so a `CardDetail` left open while the
    /// AI played was torn down and respawned once per AI action. Comparing the
    /// panels' own view values instead is the discipline the board layer
    /// already applies to its tiles: a panel whose content did not change is
    /// never respawned.
    signature: Vec<String>,
}

/// A comparable dump of everything `kind` would draw.
///
/// Every view type in [`model`] derives `Debug`, so its formatting is a total,
/// cheap fingerprint — and it is only ever computed while a panel is open.
fn panel_signature(session: &Session, kind: &PanelKind) -> String {
    let state = &session.state;
    let registry = &session.registry;
    match kind {
        PanelKind::ActionMenu { instance_id } => format!(
            "{:?}",
            model::action_menu_view(state, registry, &session.valid, instance_id)
        ),
        PanelKind::CaptainMenu { player } => format!(
            "{:?}",
            model::captain_menu_view(state, registry, &session.valid, *player, session.human)
        ),
        PanelKind::ShipMenu {
            instance_id,
            is_you,
        } => format!(
            "{:?}",
            model::ship_menu_view(state, registry, &session.valid, instance_id, *is_you)
        ),
        PanelKind::CardDetail {
            def_id,
            instance_id,
        } => format!(
            "{:?}",
            model::card_detail_view(state, registry, def_id, instance_id.as_deref())
        ),
        PanelKind::Confirm {
            instance_id,
            is_ship,
        } => format!(
            "{:?}",
            model::confirm_view(state, registry, instance_id, *is_ship)
        ),
        PanelKind::Counter => format!("{:?}", model::counter_view(state, registry, &session.valid)),
    }
}

fn close_all_panels(
    mut commands: Commands,
    mut cache: ResMut<PanelCache>,
    roots: Query<Entity, With<PanelRoot>>,
) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    cache.key = None;
    cache.signature.clear();
}

#[allow(clippy::too_many_arguments)]
fn rebuild_panels(
    mut commands: Commands,
    mut changed: MessageReader<StateChanged>,
    mut cache: ResMut<PanelCache>,
    session: Res<Session>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<crate::selection::SelectedHandCard>,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    symbols: Res<SymbolFont>,
    assets: Res<AssetServer>,
    mut art: ResMut<ArtCache>,
    anchors: Res<BoardAnchors>,
    windows: Query<&Window, With<PrimaryWindow>>,
    roots: Query<Entity, With<PanelRoot>>,
) {
    let dirty = changed.read().count() > 0;
    let window = windows
        .single()
        .map(|w| Vec2::new(w.width(), w.height()))
        .unwrap_or(Vec2::new(L::WINDOW_W, L::WINDOW_H));

    let wanted = active_panels(&mode, session.in_counter_window());
    let same_panels = cache.key.as_ref() == Some(&wanted);
    if same_panels && !dirty {
        return;
    }
    let signature: Vec<String> = wanted
        .iter()
        .map(|kind| panel_signature(&session, kind))
        .collect();
    if same_panels && cache.signature == signature {
        // The state moved, but nothing these panels show did.
        return;
    }
    cache.key = Some(wanted.clone());
    cache.signature = signature;

    for entity in &roots {
        commands.entity(entity).despawn();
    }

    let ctx = PanelCtx {
        palette: &palette,
        fonts: &fonts,
        symbols: &symbols.0,
    };
    let mut z = L::Z_MODAL;
    // Every spawner answers whether it actually drew anything: the subject of a
    // panel can be gone by the time it is built (the unit backing an open
    // action menu is KO'd by an opposing effect, or bounced), and each of them
    // gives up *before* the `PanelRoot` that carries the outside-click catcher.
    // The cache is keyed on the mode, so nothing would ever retry: the popover
    // would simply be missing while `UiMode` stayed latched on it, with the
    // `✕ Annuler` button as the only way out.
    let mut drawn = true;
    for kind in &wanted {
        drawn &= match kind {
            PanelKind::ActionMenu { instance_id } => spawn_action_menu(
                &mut commands,
                &ctx,
                &session,
                &anchors,
                window,
                z,
                instance_id,
            ),
            PanelKind::CaptainMenu { player } => {
                spawn_captain_menu(&mut commands, &ctx, &session, z, *player)
            }
            PanelKind::ShipMenu {
                instance_id,
                is_you,
            } => spawn_ship_menu(
                &mut commands,
                &ctx,
                &session,
                &assets,
                &mut art,
                z,
                instance_id,
                *is_you,
            ),
            PanelKind::CardDetail {
                def_id,
                instance_id,
            } => spawn_card_detail(
                &mut commands,
                &ctx,
                &session,
                &assets,
                &mut art,
                z,
                def_id,
                instance_id.as_deref(),
            ),
            PanelKind::Confirm {
                instance_id,
                is_ship,
            } => spawn_confirm(&mut commands, &ctx, &session, z, instance_id, *is_ship),
            PanelKind::Counter => spawn_counter_window(&mut commands, &ctx, &session, z),
        };
        z += 4;
    }

    if !drawn {
        // Back to idle, and forget the key so the next mode is rebuilt from
        // scratch rather than short-circuited as "already drawn".
        warn!("a panel had no subject left to draw: returning to idle");
        *mode = UiMode::Idle;
        selected.0 = None;
        cache.key = None;
        cache.signature.clear();
    }
}

// ============================================================
// Click routing
// ============================================================

/// The one observer behind every panel button.
///
/// A node carrying a [`ClickCommand`] applies it (mode change, or dispatch +
/// `resetUI`) and stops the click; a node marked
/// [`SwallowClicks`](widgets::SwallowClicks) only stops it, so the scrim
/// underneath does not close the panel.
fn route_panel_clicks(
    mut click: On<Pointer<Click>>,
    targets: Query<(Option<&ClickCommand>, Has<widgets::SwallowClicks>)>,
    mut mode: ResMut<UiMode>,
    mut selected: ResMut<SelectedHandCard>,
    mut dispatch: MessageWriter<DispatchAction>,
) {
    let Ok((command, swallow)) = targets.get(click.entity) else {
        return;
    };
    match command {
        Some(ClickCommand(command)) => {
            if click.button != PointerButton::Primary {
                return;
            }
            let command = command.clone();
            click.propagate(false);
            if let Some(action) = apply_ui_command(command, &mut mode, &mut selected) {
                dispatch.write(DispatchAction(action));
            }
        }
        None if swallow => click.propagate(false),
        None => {}
    }
}

/// Light a button while the pointer is over it.
///
/// A gradient skin has to be re-painted as a gradient: `BackgroundGradient`
/// covers `BackgroundColor`, so writing the colour alone would leave the
/// mock-up's gold→amber and pink calls to action visually inert on hover.
fn highlight_panel_buttons(
    buttons: Query<
        (
            &Interaction,
            &PanelButton,
            &mut BackgroundColor,
            Option<&mut BackgroundGradient>,
        ),
        Changed<Interaction>,
    >,
) {
    for (interaction, button, mut background, gradient) in buttons {
        let hovered = !matches!(interaction, Interaction::None);
        match (button.stops(hovered), gradient) {
            (Some((from, to)), Some(mut ramp)) => *ramp = button_gradient(from, to),
            _ => {
                background.0 = if hovered { button.hover } else { button.base };
            }
        }
    }
}

// ============================================================
// Shared building blocks
// ============================================================

#[allow(clippy::too_many_arguments)]
fn spawn_modal(
    commands: &mut Commands,
    ctx: &PanelCtx,
    z: i32,
    width: f32,
    edge: Color,
    name: &'static str,
    dismiss: Dismiss,
    build: impl FnOnce(&mut ChildSpawnerCommands),
) {
    let mut root = commands.spawn((PanelRoot, Name::new(name), scrim(ctx.palette, z)));
    match dismiss {
        // TS: the backdrop of every modal is `onClick={onClose}`.
        Dismiss::OnBackdrop => {
            root.insert(ClickCommand(UiCommand::Reset));
        }
        // TS `renderCounterWindow` has no backdrop handler: the player must
        // answer the attack.
        Dismiss::Never => {
            root.insert(widgets::SwallowClicks);
        }
    }
    root.with_children(|scrim| {
        scrim
            .spawn(panel(ctx.palette, width, edge))
            .observe(scroll_panel)
            .with_children(build);
    });
}

/// Move a panel body with the wheel.
///
/// `Overflow::scroll_y` only tells the layout to clip and offset by
/// [`ScrollPosition`]; nothing writes that position, so without this a modal
/// taller than the window is exactly as unreachable as a clipped one. The
/// offset is clamped to the content that actually overflows.
fn scroll_panel(
    scroll: On<Pointer<Scroll>>,
    mut panels: Query<(&mut ScrollPosition, &ComputedNode), With<widgets::ScrollArea>>,
) {
    let Ok((mut position, computed)) = panels.get_mut(scroll.entity) else {
        return;
    };
    let step = match scroll.unit {
        MouseScrollUnit::Line => widgets::SCROLL_LINE,
        MouseScrollUnit::Pixel => 1.0,
    };
    let inv = computed.inverse_scale_factor();
    let overflow = (computed.content_size().y - computed.size().y).max(0.0) * inv;
    if overflow <= 0.0 {
        position.0.y = 0.0;
        return;
    }
    position.0.y = (position.0.y - scroll.y * step).clamp(0.0, overflow);
}

/// Does clicking outside the panel close it?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dismiss {
    OnBackdrop,
    Never,
}

/// The card frame, with the instance's *effective* ATK / DEF.
/// Which card to draw and at which size — the *content* half of
/// [`spawn_face`]'s arguments, kept apart from the spawning context.
struct FaceSpec<'a> {
    def: &'a CardDef,
    instance: Option<&'a CardInstance>,
    /// Effective ATK (buffs / haki included), not `def.atk`.
    atk: i32,
    /// Effective DEF.
    def_value: i32,
    width: f32,
}

fn spawn_face(
    parent: &mut ChildSpawnerCommands,
    ctx: &PanelCtx,
    assets: &AssetServer,
    art: &mut ArtCache,
    spec: &FaceSpec,
) {
    let FaceSpec {
        def,
        instance,
        atk,
        def_value,
        width,
    } = *spec;
    let flipped = instance.and_then(|i| i.is_awakened).unwrap_or(false);
    let handle = art.card(assets, &def.id, flipped);
    let face = CardFace {
        atk,
        def_value,
        ..CardFace::from_def(def, instance, width)
    };
    let face_ctx = FaceCtx {
        palette: ctx.palette,
        fonts: ctx.fonts,
        symbols: ctx.symbols,
        art: handle,
    };
    parent
        .spawn((
            Node {
                flex_shrink: 0.,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|holder| {
            spawn_card_face(holder, &face_ctx, &face);
        });
}

/// A wrapping row of status pills — TS `<StatusBadges />`.
fn spawn_status_pills(parent: &mut ChildSpawnerCommands, ctx: &PanelCtx, chips: &[StatusChip]) {
    if chips.is_empty() {
        return;
    }
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(4.),
                row_gap: px(3.),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|pills| {
            for chip in chips {
                let color = crate::board::style::status_color(chip.effect);
                spawn_pill(
                    pills,
                    &ctx.fonts.oswald_bold,
                    format!("{} {}", chip.label, turns_label(chip.turns)),
                    color,
                    color.with_alpha(0.20),
                    L::FS_TINY + 1.0,
                );
            }
        });
}

/// A row of tiny neutral pills (traits, flags).
fn spawn_flag_pills<S: AsRef<str>>(
    parent: &mut ChildSpawnerCommands,
    ctx: &PanelCtx,
    labels: &[S],
    fg: Color,
    bg: Color,
) {
    if labels.is_empty() {
        return;
    }
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(4.),
                row_gap: px(3.),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|pills| {
            for label in labels {
                spawn_pill(
                    pills,
                    &ctx.fonts.oswald,
                    label.as_ref().to_string(),
                    fg,
                    bg,
                    L::FS_TINY + 1.0,
                );
            }
        });
}

/// A titled box with one text line per entry.
fn spawn_list_section(
    parent: &mut ChildSpawnerCommands,
    ctx: &PanelCtx,
    caption: &str,
    caption_color: Color,
    tint: Color,
    text_color: Color,
    lines: &[String],
) {
    if lines.is_empty() {
        return;
    }
    parent.spawn(section(tint, None)).with_children(|entries| {
        spawn_caption(entries, ctx, caption.to_string(), caption_color);
        for value in lines {
            entries.spawn(body(
                value.clone(),
                &ctx.fonts.spectral,
                L::FS_STAT,
                text_color,
            ));
        }
    });
}

/// `⚔ 12   ⛨ 8` — the stat pair used by the action and captain menus.
fn spawn_stat_pair(parent: &mut ChildSpawnerCommands, ctx: &PanelCtx, atk: i32, def_value: i32) {
    parent
        .spawn((row(8.), Pickable::IGNORE))
        .with_children(|stats| {
            stats
                .spawn((row(2.), Pickable::IGNORE))
                .with_children(|pair| {
                    pair.spawn(line(SWORD, ctx.symbols, L::FS_STAT + 1.0, ctx.palette.atk));
                    pair.spawn(line(
                        atk.to_string(),
                        &ctx.fonts.oswald_bold,
                        L::FS_NAME,
                        ctx.palette.atk,
                    ));
                });
            stats
                .spawn((row(2.), Pickable::IGNORE))
                .with_children(|pair| {
                    pair.spawn(line(SHIELD, ctx.symbols, L::FS_STAT + 1.0, ctx.palette.def));
                    pair.spawn(line(
                        def_value.to_string(),
                        &ctx.fonts.oswald_bold,
                        L::FS_NAME,
                        ctx.palette.def,
                    ));
                });
        });
}

// ============================================================
// Action menu (ActionMenu.tsx)
// ============================================================

/// Where a popover sits, so it can follow its tile when the board re-lays
/// itself out (a window resize scales every slot).
#[derive(Component, Debug, Clone)]
pub struct PopoverAnchor {
    /// The instance id whose tile this popover belongs to.
    pub instance_id: String,
    /// Which edge the caret is currently on, so `place_popovers` only moves it
    /// when the panel actually flips sides.
    pub above: bool,
}

/// The `.pop:after` caret, so a flip can move it to the other edge.
#[derive(Component, Debug, Clone, Copy)]
pub struct PopoverCaret;

/// The caret's box: on the bottom edge when the popover floats above its tile,
/// on the top edge when it was flipped below.
fn caret_node(above: bool) -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: px(L::POPOVER_CARET),
        height: px(L::POPOVER_CARET),
        left: percent(50.),
        bottom: if above {
            px(-L::POPOVER_CARET * 0.5)
        } else {
            Val::Auto
        },
        top: if above {
            Val::Auto
        } else {
            px(-L::POPOVER_CARET * 0.5)
        },
        ..default()
    }
}

/// Place `size`-wide popover over the tile `anchor`, inside `window`.
///
/// Mock-up `.pop{left:50%;bottom:68px;transform:translateX(-50%)}` — centred on
/// the tile and floating just above it, flipping below when the tile is too
/// close to the top edge.
pub fn popover_placement(anchor: Rect, size: Vec2, window: Vec2) -> (Vec2, bool) {
    let margin = 8.0;
    let left = (anchor.center().x - size.x * 0.5)
        .max(margin)
        .min((window.x - size.x - margin).max(margin));
    let above = anchor.min.y - size.y - L::POPOVER_GAP;
    if above >= margin {
        (Vec2::new(left, above), true)
    } else {
        (
            Vec2::new(left, (anchor.max.y + L::POPOVER_GAP).min(window.y - margin)),
            false,
        )
    }
}

/// The unit popover — mock-up `.pop`, the client's core interaction
/// ("Carte cliquée = infos + actions · sinon pleine illustration").
///
/// It is **not** a modal: there is no scrim and no 552 px centred body, because
/// the whole point is that the board stays visible around the tile you are
/// acting on. A transparent full-screen catcher takes the outside click, the
/// popover itself swallows its own.
#[allow(clippy::too_many_arguments)]
fn spawn_action_menu(
    commands: &mut Commands,
    ctx: &PanelCtx,
    session: &Session,
    anchors: &BoardAnchors,
    window: Vec2,
    z: i32,
    instance_id: &str,
) -> bool {
    let Some(view) = model::action_menu_view(
        &session.state,
        &session.registry,
        &session.valid,
        instance_id,
    ) else {
        return false;
    };
    let Some(instance) = session.state.card(instance_id) else {
        return false;
    };
    let Some(def) = session.registry.card_def(&instance.def_id) else {
        return false;
    };
    let pv = instance.current_pv;
    let cost = def.cost;
    let palette = *ctx.palette;
    let anchor = anchors
        .instance(instance_id)
        // No anchor yet (the tile is laid out next frame): park it centred, the
        // per-frame placement pass moves it as soon as the rectangle exists.
        .unwrap_or_else(|| Rect::from_center_size(window * 0.5, Vec2::splat(1.0)));
    let (at, above) = popover_placement(anchor, Vec2::new(L::POPOVER_W, 120.0), window);

    commands
        .spawn((
            PanelRoot,
            Name::new("ActionPopover"),
            catcher(z),
            // Clicking the board around the popover closes it.
            ClickCommand(UiCommand::Reset),
        ))
        .with_children(|root| {
            let mut pop = root.spawn((
                popover(&palette, at),
                PopoverAnchor {
                    instance_id: instance_id.to_string(),
                    above,
                },
            ));
            pop.with_children(|pop| {
                // The `:after` caret — a rotated square peeking out of the edge
                // that faces the tile. Mock-up `.pop:after{bottom:-7px}` is a
                // downward caret, which is only right while `.pop` is *above*
                // its tile; `popover_placement` flips the panel below when the
                // tile is near the top of the window, and the caret has to
                // follow or it points away from what it belongs to.
                pop.spawn((
                    PopoverCaret,
                    caret_node(above),
                    UiTransform {
                        translation: Val2::px(-L::POPOVER_CARET * 0.5, 0.),
                        rotation: Rot2::degrees(45.),
                        ..default()
                    },
                    BackgroundColor(palette.gold.with_alpha(0.5)),
                    Pickable::IGNORE,
                ));
                // `.ph` — cost disc + name, and the escape hatch.
                pop.spawn((row(6.), Pickable::IGNORE))
                    .with_children(|head| {
                        head.spawn((
                            Node {
                                width: px(18.),
                                height: px(18.),
                                flex_shrink: 0.,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                border_radius: BorderRadius::MAX,
                                ..default()
                            },
                            BackgroundGradient::from(LinearGradient::to_bottom(vec![
                                ColorStop::new(palette.gold, percent(0.)),
                                ColorStop::new(palette.gold_deep, percent(100.)),
                            ])),
                            Pickable::IGNORE,
                            children![line(
                                cost.to_string(),
                                &ctx.fonts.poppins_bold,
                                L::FS_LABEL,
                                palette.text_on_gold,
                            )],
                        ));
                        // The name opens the full card — TS "Détails".
                        head.spawn((
                            Node {
                                flex_grow: 1.,
                                min_width: px(0.),
                                ..default()
                            },
                            ClickCommand(view.detail_command.clone()),
                            children![line(
                                view.name.clone(),
                                &ctx.fonts.cinzel_bold,
                                L::FS_NAME,
                                palette.text,
                            )],
                        ));
                        spawn_close_cross(head, ctx);
                    });

                // `.stats` — ⚔ / 🛡 / ❤, the only numbers the mock-up shows.
                pop.spawn((row(8.), Pickable::IGNORE))
                    .with_children(|stats| {
                        for (glyph, value, color) in [
                            (SWORD, view.atk, palette.atk),
                            (SHIELD, view.def, palette.def),
                            (HEART, pv, palette.hp),
                        ] {
                            stats
                                .spawn((row(3.), Pickable::IGNORE))
                                .with_children(|stat| {
                                    stat.spawn(line(glyph, ctx.symbols, L::FS_BODY, color));
                                    stat.spawn(line(
                                        value.to_string(),
                                        &ctx.fonts.poppins_bold,
                                        L::FS_BODY,
                                        color,
                                    ));
                                });
                        }
                    });

                if !view.flags.is_empty() {
                    pop.spawn((row(4.), Pickable::IGNORE)).with_children(|f| {
                        for flag in &view.flags {
                            spawn_pill(
                                f,
                                &ctx.fonts.poppins,
                                *flag,
                                palette.amber,
                                palette.amber.with_alpha(0.18),
                                L::FS_TINY,
                            );
                        }
                    });
                }

                // §8.36/§8.38/§8.40 — "perd N PV permanent (Sable)" and the
                // `noHeal` blow. Both change what the PV number *means*, so
                // they are stated next to it rather than left to the badge row.
                if view.pv_max_loss > 0 || view.no_heal {
                    let mut notes: Vec<String> = Vec::new();
                    if view.pv_max_loss > 0 {
                        notes.push(format!("PV max \u{2212}{} (permanent)", view.pv_max_loss));
                    }
                    if view.no_heal {
                        notes.push("Soins bloqués".to_string());
                    }
                    pop.spawn(line(
                        notes.join(" \u{00B7} "),
                        &ctx.fonts.poppins,
                        L::FS_TINY,
                        palette.hp_low,
                    ));
                }

                // §8.38 — the Taunt shrank this unit's target list to one, and
                // the board gives no hint of that on its own.
                if let Some(taunt) = &view.taunt {
                    pop.spawn(line(
                        taunt.clone(),
                        &ctx.fonts.poppins,
                        L::FS_TINY,
                        palette.impact,
                    ));
                }

                // `.acts` — "Attaquer" (gold) and "Spéciale ★" (red).
                pop.spawn((row(6.), Pickable::IGNORE))
                    .with_children(|acts| {
                        let mut any = false;
                        if let Some(base) = &view.base {
                            any = true;
                            spawn_popover_action(acts, ctx, base, "Attaquer");
                        }
                        // A support character has both: its printed effect
                        // *and* the attack above (`actions.rs` offers the two
                        // in the same turn).
                        if let Some(support) = &view.support {
                            any = true;
                            let label = support.name.clone();
                            spawn_popover_action(acts, ctx, support, &label);
                        }
                        if let Some(special) = &view.special {
                            any = true;
                            spawn_popover_action(acts, ctx, special, "Spéciale");
                        }
                        if !any {
                            acts.spawn(line(
                                "Aucune action",
                                &ctx.fonts.poppins,
                                L::FS_LABEL,
                                palette.text_faint,
                            ));
                        }
                    });

                // The awakened fruit's own special — `fruitSpecialAttack`
                // (§8.28 follow-up × §8.48). Its name is the button, because a
                // unit may wear more than one and "Spéciale" would not say which.
                for fruit in &view.fruit_specials {
                    let label = fruit.name.clone();
                    pop.spawn((row(6.), Pickable::IGNORE))
                        .with_children(|acts| {
                            spawn_popover_action(acts, ctx, fruit, &label);
                        });
                }

                // *Éveiller* (`awakenFruit`) and *Déplacer* (`moveCharacter`).
                for extra in view.awakenings.iter().chain(view.free_move.as_ref()) {
                    pop.spawn((row(6.), Pickable::IGNORE))
                        .with_children(|acts| {
                            spawn_popover_ability(acts, ctx, extra);
                        });
                }

                if !view.equipment.is_empty() {
                    let equipment: Vec<String> = view.equipment.iter().map(|e| e.label()).collect();
                    pop.spawn(line(
                        equipment.join(" · "),
                        &ctx.fonts.poppins,
                        L::FS_TINY,
                        palette.amber.with_alpha(0.9),
                    ));
                }
            });
        });
    true
}

/// One extra `.acts button` — *Éveiller* / *Déplacer*, the rows that carry an
/// [`model::AbilityButton`] rather than an [`AttackOption`].
fn spawn_popover_ability(
    parent: &mut ChildSpawnerCommands,
    ctx: &PanelCtx,
    ability: &model::AbilityButton,
) {
    let label = match &ability.reason {
        Some(reason) => format!("{} \u{2014} {reason}", ability.label),
        None if ability.cost > 0 => {
            format!("{} \u{2014} {} Volont\u{00E9}", ability.label, ability.cost)
        }
        None => ability.label.clone(),
    };
    let mut spec = ButtonSpec::new(label, ButtonTone::Ghost, ability.command.clone())
        .disabled(ability.disabled)
        .grow();
    spec.height = 28.0;
    spec.font_size = L::FS_LABEL;
    spawn_button(parent, ctx, spec);
}

/// One `.acts button`: the mock-up's two gradient calls to action.
fn spawn_popover_action(
    parent: &mut ChildSpawnerCommands,
    ctx: &PanelCtx,
    option: &AttackOption,
    label: &str,
) {
    let tone = if option.is_special {
        ButtonTone::Danger
    } else {
        ButtonTone::Gold
    };
    let label = match (&option.reason, option.is_special) {
        (Some(reason), _) => format!("{label} — {reason}"),
        (None, true) => format!("{label} {STAR}"),
        (None, false) => label.to_string(),
    };
    let mut spec = ButtonSpec::new(label, tone, option.command.clone())
        .disabled(option.disabled)
        .grow();
    spec.height = 28.0;
    spec.font_size = L::FS_LABEL;
    spawn_button(parent, ctx, spec);
}

/// Keep every open popover on its tile.
///
/// The board publishes its anchors from the computed layout, so a tile only
/// gets a rectangle on the frame *after* it is spawned, and it moves whenever
/// the window is resized. Re-placing every frame is one query over at most one
/// entity.
fn place_popovers(
    anchors: Res<BoardAnchors>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut popovers: Query<(&mut PopoverAnchor, &ComputedNode, &Children, &mut Node)>,
    mut carets: Query<&mut Node, (With<PopoverCaret>, Without<PopoverAnchor>)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let viewport = Vec2::new(window.width(), window.height());
    for (mut anchored, computed, children, mut node) in &mut popovers {
        let Some(rect) = anchors.instance(&anchored.instance_id) else {
            continue;
        };
        let inv = computed.inverse_scale_factor();
        let size = computed.size() * inv;
        let size = if size.y > 1.0 {
            size
        } else {
            Vec2::new(L::POPOVER_W, 120.0)
        };
        let (at, above) = popover_placement(rect, size, viewport);
        let (left, top) = (px(at.x), px(at.y));
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }
        if anchored.above != above {
            anchored.above = above;
            for child in children.iter() {
                if let Ok(mut caret) = carets.get_mut(child) {
                    *caret = caret_node(above);
                }
            }
        }
    }
}

// ============================================================
// Captain menu (CaptainMenu.tsx)
// ============================================================

const CAPTAIN_MENU_W: f32 = 344.0;

fn spawn_captain_menu(
    commands: &mut Commands,
    ctx: &PanelCtx,
    session: &Session,
    z: i32,
    player: PlayerId,
) -> bool {
    let Some(view) = model::captain_menu_view(
        &session.state,
        &session.registry,
        &session.valid,
        player,
        session.human,
    ) else {
        return false;
    };
    let palette = *ctx.palette;
    let visual = faction_visual(view.faction);
    let edge = if view.flipped {
        palette.target
    } else {
        palette.gold
    };
    let hp_color = palette.hp_color(view.ratio);

    spawn_modal(
        commands,
        ctx,
        z,
        CAPTAIN_MENU_W,
        edge.with_alpha(0.65),
        "CaptainMenu",
        Dismiss::OnBackdrop,
        |panel| {
            // --- header ---
            panel
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Start,
                        column_gap: px(8.),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|header| {
                    header
                        .spawn((column(1.), Pickable::IGNORE))
                        .with_children(|names| {
                            names.spawn(line(
                                view.name.clone(),
                                &ctx.fonts.cinzel_bold,
                                L::FS_NAME + 3.0,
                                palette.text,
                            ));
                            names.spawn((
                                line(
                                    format!("{} \u{00B7} Capitaine", visual.label),
                                    &ctx.fonts.oswald,
                                    L::FS_TINY + 1.0,
                                    visual.accent,
                                ),
                                bevy::text::LetterSpacing::Px(1.4),
                            ));
                        });
                    spawn_pill(
                        header,
                        &ctx.fonts.oswald_bold,
                        if view.flipped {
                            "Verso".to_string()
                        } else {
                            format!("{STAR} Recto")
                        },
                        palette.bg_deep,
                        if view.flipped {
                            palette.target
                        } else {
                            palette.gold
                        },
                        L::FS_TINY + 1.0,
                    );
                });

            // --- PV + stats ---
            panel
                .spawn((row(10.), Pickable::IGNORE))
                .with_children(|stats| {
                    stats
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: px(2.),
                                flex_grow: 1.,
                                min_width: px(0.),
                                ..default()
                            },
                            Pickable::IGNORE,
                        ))
                        .with_children(|gauge| {
                            spawn_gauge(gauge, view.ratio, hp_color, 8.);
                            gauge.spawn(line(
                                format!("{} / {} PV", view.pv, view.max_pv),
                                &ctx.fonts.oswald_bold,
                                L::FS_LABEL,
                                hp_color,
                            ));
                        });
                    spawn_stat_pair(stats, ctx, view.atk, view.def);
                });

            spawn_status_pills(panel, ctx, &view.statuses);

            // --- passive of the visible side ---
            panel
                .spawn(section(palette.gold.with_alpha(0.10), None))
                .with_children(|passive| {
                    passive
                        .spawn((row(4.), Pickable::IGNORE))
                        .with_children(|head| {
                            head.spawn(line(SPARKLE, ctx.symbols, L::FS_STAT, palette.gold));
                            head.spawn(line(
                                view.passive_name.clone(),
                                &ctx.fonts.spectral_bold,
                                L::FS_BODY,
                                palette.amber,
                            ));
                        });
                    passive.spawn(body(
                        view.passive_description.clone(),
                        &ctx.fonts.spectral,
                        L::FS_LABEL,
                        palette.text_dim,
                    ));
                });

            // --- powers ---
            for ability in &view.abilities {
                spawn_ability_row(panel, ctx, ability);
            }

            if !view.natural_haki.is_empty() {
                let labels: Vec<&str> = view.natural_haki.iter().map(|h| haki_label(*h)).collect();
                panel.spawn(line(
                    format!("{EYE} Haki naturel : {}", labels.join(", ")),
                    &ctx.fonts.oswald,
                    L::FS_TINY + 1.0,
                    Color::srgb(0.72, 0.62, 0.92),
                ));
            }
            if !view.traits.is_empty() {
                let labels: Vec<&str> = view.traits.iter().map(|t| trait_label(*t)).collect();
                spawn_flag_pills(
                    panel,
                    ctx,
                    &labels,
                    palette.text,
                    Color::srgba(1., 1., 1., 0.12),
                );
            }

            // §8.28 follow-up — the captain wears the fruit printed for it.
            if !view.equipment.is_empty() {
                let equipment: Vec<String> = view.equipment.iter().map(|e| e.label()).collect();
                panel.spawn(line(
                    format!("{ANCHOR} {}", equipment.join(" \u{00B7} ")),
                    &ctx.fonts.oswald,
                    L::FS_TINY + 1.0,
                    palette.amber.with_alpha(0.9),
                ));
            }

            // --- what engaging unlocks ---
            if let Some(preview) = &view.verso_preview {
                panel
                    .spawn(section(
                        palette.target.with_alpha(0.12),
                        Some(palette.target.with_alpha(0.35)),
                    ))
                    .with_children(|preview_box| {
                        spawn_caption(
                            preview_box,
                            ctx,
                            "En Verso (engagé)",
                            Color::srgb(0.95, 0.55, 0.55),
                        );
                        preview_box.spawn(body(
                            format!(
                                "{} ATK \u{00B7} {} DEF \u{00B7} {} PV \u{2014} {SPARKLE} {}",
                                preview.atk, preview.def, preview.pv, preview.passive_name
                            ),
                            &ctx.fonts.spectral,
                            L::FS_STAT,
                            palette.text.with_alpha(0.82),
                        ));
                    });
            }

            // --- actions ---
            panel
                .spawn((column(6.), Pickable::IGNORE))
                .with_children(|actions| {
                    if view.is_you {
                        if view.can_flip {
                            // §8.6 / §8.33: the flip is free while one of the
                            // five printed clauses holds, and costs
                            // `flipCondition.cost ?? 0` otherwise. The player
                            // has to be able to see which, or a "free" flip
                            // looks like the engine losing Volonté.
                            let label = match (view.free_flip_reason, view.flip_cost) {
                                (Some(reason), _) => {
                                    format!("Engager le Capitaine \u{2014} {reason}")
                                }
                                (None, 0) => "Engager le Capitaine \u{2014} gratuit".to_string(),
                                (None, cost) => {
                                    format!("Engager le Capitaine \u{2014} {cost} Volont\u{00E9}")
                                }
                            };
                            spawn_button(
                                actions,
                                ctx,
                                ButtonSpec::new(
                                    label,
                                    ButtonTone::Danger,
                                    view.flip_command.clone(),
                                )
                                .glyph(SWORD),
                            );
                        }
                        if view.show_attack {
                            let label = match (view.can_attack, view.attack_reason) {
                                (false, Some(reason)) => format!("Attaquer \u{2014} {reason}"),
                                _ => "Attaquer".to_string(),
                            };
                            spawn_button(
                                actions,
                                ctx,
                                ButtonSpec::new(
                                    label,
                                    ButtonTone::Danger,
                                    view.attack_command.clone(),
                                )
                                .glyph(SWORD)
                                .disabled(!view.can_attack),
                            );
                        }
                        // The ★ special attack the engine offers on the same
                        // `captainAttack` action (`isSpecial: true`). Its
                        // `AbilityRow` above already shows the cost; this is
                        // the only way to actually spend it.
                        if view.show_special_attack {
                            let label = match (view.can_special_attack, view.special_attack_reason)
                            {
                                (false, Some(reason)) => {
                                    format!("{} \u{2014} {reason}", view.special_attack_name)
                                }
                                _ => format!(
                                    "{} \u{2014} {} Volont\u{00E9}",
                                    view.special_attack_name, view.special_attack_cost
                                ),
                            };
                            spawn_button(
                                actions,
                                ctx,
                                ButtonSpec::new(
                                    label,
                                    ButtonTone::Gold,
                                    view.special_attack_command.clone(),
                                )
                                .glyph(STAR)
                                .disabled(!view.can_special_attack),
                            );
                        }
                        // §8.34(b) — `useSurcharge`. Only ever drawn when the
                        // active face prints a `surcharge`, so it is absent on
                        // the whole shipped catalogue and appears the day data
                        // defines one.
                        if let Some(surcharge) = &view.surcharge {
                            let label = match (&surcharge.reason, surcharge.cost) {
                                (Some(reason), _) => {
                                    format!("{} \u{2014} {reason}", surcharge.label)
                                }
                                (None, cost) => {
                                    format!("{} \u{2014} {cost} Volont\u{00E9}", surcharge.label)
                                }
                            };
                            spawn_button(
                                actions,
                                ctx,
                                ButtonSpec::new(label, ButtonTone::Gold, surcharge.command.clone())
                                    .glyph(BOLT)
                                    .disabled(surcharge.disabled),
                            );
                        }
                        // §8.28 follow-up × §8.48 — the fruit the captain wears.
                        for fruit in &view.fruit_specials {
                            let label = match (&fruit.reason, fruit.cost) {
                                (Some(reason), _) => format!("{} \u{2014} {reason}", fruit.name),
                                (None, cost) => {
                                    format!("{} \u{2014} {cost} Volont\u{00E9}", fruit.name)
                                }
                            };
                            spawn_button(
                                actions,
                                ctx,
                                ButtonSpec::new(label, ButtonTone::Danger, fruit.command.clone())
                                    .glyph(STAR)
                                    .disabled(fruit.disabled),
                            );
                        }
                        for awakening in &view.awakenings {
                            let label = match (&awakening.reason, awakening.cost) {
                                (Some(reason), _) => {
                                    format!("{} \u{2014} {reason}", awakening.label)
                                }
                                (None, cost) => {
                                    format!("{} \u{2014} {cost} Volont\u{00E9}", awakening.label)
                                }
                            };
                            spawn_button(
                                actions,
                                ctx,
                                ButtonSpec::new(
                                    label,
                                    ButtonTone::Ghost,
                                    awakening.command.clone(),
                                )
                                .glyph(SPARKLE)
                                .disabled(awakening.disabled),
                            );
                        }
                        if view.can_king_haki {
                            spawn_button(
                                actions,
                                ctx,
                                ButtonSpec::new(
                                    "Haki des Rois",
                                    ButtonTone::Gold,
                                    view.king_haki_command.clone(),
                                )
                                .glyph(CROWN),
                            );
                        }
                    }
                    spawn_button(
                        actions,
                        ctx,
                        ButtonSpec::new("Fermer", ButtonTone::Ghost, UiCommand::Reset),
                    );
                });
        },
    );
    true
}

/// TS `<AbilityRow />`.
fn spawn_ability_row(parent: &mut ChildSpawnerCommands, ctx: &PanelCtx, ability: &AbilityRow) {
    let palette = ctx.palette;
    let (glyph, accent) = match ability.kind {
        AbilityKind::Base => (SWORD, palette.deploy),
        AbilityKind::Attack => (STAR, palette.atk),
        AbilityKind::Surcharge => (BOLT, palette.gold),
    };
    let ability = ability.clone();
    parent
        .spawn(section(Color::srgba(1., 1., 1., 0.05), None))
        .with_children(|node| {
            node.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: px(6.),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|head| {
                head.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        align_items: AlignItems::Center,
                        column_gap: px(5.),
                        row_gap: px(2.),
                        flex_shrink: 1.,
                        min_width: px(0.),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|left| {
                    left.spawn(line(glyph, ctx.symbols, L::FS_STAT, accent));
                    left.spawn(line(
                        ability.name.clone(),
                        &ctx.fonts.spectral_bold,
                        L::FS_BODY + 0.5,
                        palette.text,
                    ));
                    if let Some(element) = ability.element {
                        spawn_pill(
                            left,
                            &ctx.fonts.oswald,
                            element_label(element),
                            Color::WHITE,
                            element_color(element),
                            L::FS_TINY + 0.5,
                        );
                    }
                });
                head.spawn(line(
                    format!("{} Vol.", ability.cost),
                    &ctx.fonts.oswald,
                    L::FS_LABEL,
                    palette.text_dim,
                ));
            });
            // §8.34(c) — a recto attack / surcharge is printed but unplayable.
            if let Some(reason) = ability.reason {
                node.spawn(line(
                    reason,
                    &ctx.fonts.oswald,
                    L::FS_TINY + 1.0,
                    palette.hp_low,
                ));
            }
            if let Some(description) = &ability.description {
                node.spawn(body(
                    description.clone(),
                    &ctx.fonts.spectral,
                    L::FS_LABEL,
                    palette.text_dim,
                ));
            }
        });
}

// ============================================================
// Ship menu (ShipMenu.tsx)
// ============================================================

const SHIP_MENU_W: f32 = 300.0;
const SHIP_CARD_W: f32 = 258.0;

#[allow(clippy::too_many_arguments)]
fn spawn_ship_menu(
    commands: &mut Commands,
    ctx: &PanelCtx,
    session: &Session,
    assets: &AssetServer,
    art: &mut ArtCache,
    z: i32,
    instance_id: &str,
    is_you: bool,
) -> bool {
    let Some(view) = model::ship_menu_view(
        &session.state,
        &session.registry,
        &session.valid,
        instance_id,
        is_you,
    ) else {
        return false;
    };
    let Some(instance) = session.state.card(instance_id).cloned() else {
        return false;
    };
    let Some(def) = session.registry.card_def(&instance.def_id).cloned() else {
        return false;
    };
    let cyan = Color::srgb(0.36, 0.78, 0.88);

    spawn_modal(
        commands,
        ctx,
        z,
        SHIP_MENU_W,
        cyan.with_alpha(0.45),
        "ShipMenu",
        Dismiss::OnBackdrop,
        |panel| {
            panel
                .spawn((
                    Node {
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|header| {
                    header
                        .spawn((row(4.), Pickable::IGNORE))
                        .with_children(|left| {
                            left.spawn(line(ANCHOR, ctx.symbols, L::FS_BODY, cyan));
                            left.spawn((
                                line(
                                    "Navire",
                                    &ctx.fonts.oswald_bold,
                                    L::FS_LABEL,
                                    cyan.with_alpha(0.85),
                                ),
                                bevy::text::LetterSpacing::Px(1.4),
                            ));
                        });
                    if let Some(active) = &view.active {
                        header.spawn(line(
                            format!("{} Vol.", active.cost),
                            &ctx.fonts.oswald_bold,
                            L::FS_NAME,
                            cyan,
                        ));
                    }
                });

            spawn_face(
                panel,
                ctx,
                assets,
                art,
                &FaceSpec {
                    def: &def,
                    instance: Some(&instance),
                    atk: def.atk.unwrap_or(0),
                    def_value: def.def.unwrap_or(0),
                    width: SHIP_CARD_W,
                },
            );

            if view.is_you
                && let Some(active) = &view.active
            {
                let label = match (view.can_activate, view.reason) {
                    (false, Some(reason)) => {
                        format!("Activer \u{2014} {} ({reason})", active.name)
                    }
                    _ => format!("Activer \u{2014} {}", active.name),
                };
                spawn_button(
                    panel,
                    ctx,
                    ButtonSpec::new(label, ButtonTone::Cyan, view.activate_command.clone())
                        .glyph(ANCHOR)
                        .disabled(!view.can_activate),
                );
            }

            // The web reaches a ship's card detail from a header button
            // (`⚓ <name>`); the header now shows the foe's ship only in its
            // command bar, so the route lives here — this menu is what a click
            // on either side's ship slot opens.
            panel
                .spawn((row(8.), Pickable::IGNORE))
                .with_children(|actions| {
                    spawn_button(
                        actions,
                        ctx,
                        ButtonSpec::new(
                            "Détails",
                            ButtonTone::Ghost,
                            UiCommand::SetMode(UiMode::CardDetail {
                                def_id: view.def_id.clone(),
                                instance_id: Some(view.instance_id.clone()),
                            }),
                        )
                        .glyph(EYE)
                        .grow(),
                    );
                    spawn_button(
                        actions,
                        ctx,
                        ButtonSpec::new("Fermer", ButtonTone::Ghost, UiCommand::Reset).grow(),
                    );
                });
        },
    );
    true
}

// ============================================================
// Card detail (CardDetail.tsx)
// ============================================================

const CARD_DETAIL_W: f32 = 556.0;
const DETAIL_CARD_W: f32 = 248.0;

#[allow(clippy::too_many_arguments)]
fn spawn_card_detail(
    commands: &mut Commands,
    ctx: &PanelCtx,
    session: &Session,
    assets: &AssetServer,
    art: &mut ArtCache,
    z: i32,
    def_id: &str,
    instance_id: Option<&str>,
) -> bool {
    let Some(view) =
        model::card_detail_view(&session.state, &session.registry, def_id, instance_id)
    else {
        return false;
    };
    let Some(def) = session.registry.card_def(def_id).cloned() else {
        return false;
    };
    let instance = instance_id.and_then(|id| session.state.card(id)).cloned();
    let (atk, def_value) = match instance_id {
        Some(id) => (
            tcgop_engine::board::get_effective_atk(&session.state, &session.registry, id)
                .unwrap_or_else(|_| def.atk.unwrap_or(0)),
            tcgop_engine::board::get_effective_def(&session.state, &session.registry, id)
                .unwrap_or_else(|_| def.def.unwrap_or(0)),
        ),
        None => (def.atk.unwrap_or(0), def.def.unwrap_or(0)),
    };
    let palette = *ctx.palette;

    spawn_modal(
        commands,
        ctx,
        z,
        CARD_DETAIL_W,
        palette.gold.with_alpha(0.55),
        "CardDetail",
        Dismiss::OnBackdrop,
        |panel| {
            panel
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Start,
                        column_gap: px(14.),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|split| {
                    spawn_face(
                        split,
                        ctx,
                        assets,
                        art,
                        &FaceSpec {
                            def: &def,
                            instance: instance.as_ref(),
                            atk,
                            def_value,
                            width: DETAIL_CARD_W,
                        },
                    );

                    split
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: px(9.),
                                flex_grow: 1.,
                                min_width: px(0.),
                                ..default()
                            },
                            Pickable::IGNORE,
                        ))
                        .with_children(|col| {
                            col.spawn((
                                Node {
                                    flex_direction: FlexDirection::Row,
                                    justify_content: JustifyContent::SpaceBetween,
                                    align_items: AlignItems::Start,
                                    column_gap: px(6.),
                                    ..default()
                                },
                                Pickable::IGNORE,
                            ))
                            .with_children(|head| {
                                head.spawn((
                                    Node {
                                        flex_direction: FlexDirection::Column,
                                        row_gap: px(2.),
                                        flex_shrink: 1.,
                                        min_width: px(0.),
                                        ..default()
                                    },
                                    Pickable::IGNORE,
                                ))
                                .with_children(|titles| {
                                    titles.spawn(body(
                                        view.name.clone(),
                                        &ctx.fonts.cinzel_bold,
                                        L::FS_NAME + 2.0,
                                        palette.text,
                                    ));
                                    titles.spawn((
                                        line(
                                            view.type_line.clone(),
                                            &ctx.fonts.oswald,
                                            L::FS_LABEL,
                                            palette.text_faint,
                                        ),
                                        bevy::text::LetterSpacing::Px(1.0),
                                    ));
                                });
                                spawn_close_cross(head, ctx);
                            });

                            let traits: Vec<&str> =
                                view.traits.iter().map(|t| trait_label(*t)).collect();
                            if !traits.is_empty() {
                                col.spawn((
                                    Node {
                                        flex_direction: FlexDirection::Row,
                                        flex_wrap: FlexWrap::Wrap,
                                        column_gap: px(5.),
                                        row_gap: px(4.),
                                        ..default()
                                    },
                                    Pickable::IGNORE,
                                ))
                                .with_children(|pills| {
                                    for t in &view.traits {
                                        spawn_pill(
                                            pills,
                                            &ctx.fonts.oswald,
                                            trait_label(*t),
                                            Color::WHITE,
                                            trait_color(*t).with_alpha(0.75),
                                            L::FS_LABEL,
                                        );
                                    }
                                });
                            }

                            spawn_list_section(
                                col,
                                ctx,
                                "Synergies",
                                palette.gold,
                                palette.gold.with_alpha(0.12),
                                palette.text.with_alpha(0.82),
                                &view.synergies,
                            );
                            spawn_list_section(
                                col,
                                ctx,
                                "Effets actifs",
                                palette.text_dim,
                                Color::srgba(1., 1., 1., 0.05),
                                palette.text.with_alpha(0.82),
                                &view.statuses,
                            );
                            let equipment: Vec<String> =
                                view.equipment.iter().map(|e| e.label()).collect();
                            spawn_list_section(
                                col,
                                ctx,
                                "Équipement",
                                palette.amber,
                                palette.amber.with_alpha(0.10),
                                palette.amber.with_alpha(0.90),
                                &equipment,
                            );

                            if !view.has_side {
                                col.spawn(body(
                                    "Toutes les infos sont sur la carte.",
                                    &ctx.fonts.spectral,
                                    L::FS_BODY,
                                    palette.text_faint,
                                ));
                            }
                        });
                });
        },
    );
    true
}

// ============================================================
// Event / ship confirmation (EventConfirm.tsx)
// ============================================================

const CONFIRM_W: f32 = 420.0;

fn spawn_confirm(
    commands: &mut Commands,
    ctx: &PanelCtx,
    session: &Session,
    z: i32,
    instance_id: &str,
    is_ship: bool,
) -> bool {
    let Some(view) = model::confirm_view(&session.state, &session.registry, instance_id, is_ship)
    else {
        return false;
    };
    let palette = *ctx.palette;
    let cyan = Color::srgb(0.36, 0.78, 0.88);

    spawn_modal(
        commands,
        ctx,
        z,
        CONFIRM_W,
        palette.gold.with_alpha(0.55),
        "Confirm",
        Dismiss::OnBackdrop,
        |panel| {
            panel
                .spawn((row(6.), Pickable::IGNORE))
                .with_children(|header| {
                    header.spawn(line(
                        if view.is_ship { ANCHOR } else { SPARKLE },
                        ctx.symbols,
                        L::FS_TITLE - 2.0,
                        palette.gold,
                    ));
                    header.spawn((
                        line(
                            if view.is_ship {
                                "Déployer le navire"
                            } else {
                                "Jouer l'événement"
                            },
                            &ctx.fonts.oswald_bold,
                            L::FS_BODY,
                            palette.gold,
                        ),
                        bevy::text::LetterSpacing::Px(1.4),
                    ));
                });

            panel.spawn(body(
                view.name.clone(),
                &ctx.fonts.cinzel_bold,
                L::FS_TITLE,
                palette.text,
            ));

            panel
                .spawn((row(8.), Pickable::IGNORE))
                .with_children(|cost| {
                    spawn_pill(
                        cost,
                        &ctx.fonts.oswald_bold,
                        format!("{} Vol.", view.cost),
                        palette.def,
                        palette.def.with_alpha(0.18),
                        L::FS_STAT,
                    );
                    cost.spawn(line(
                        format!("({} disponible)", view.volonte),
                        &ctx.fonts.oswald,
                        L::FS_LABEL,
                        if view.can_afford {
                            palette.hp
                        } else {
                            palette.foe
                        },
                    ));
                });

            if let Some(effect) = &view.effect {
                spawn_text_section(
                    panel,
                    ctx,
                    "Effet",
                    palette.gold,
                    palette.gold.with_alpha(0.10),
                    effect,
                );
            }
            if let Some(counter) = &view.counter {
                spawn_text_section(
                    panel,
                    ctx,
                    "Effet Counter",
                    palette.def,
                    palette.def.with_alpha(0.10),
                    counter,
                );
            }
            if let Some(passive) = &view.ship_passive {
                spawn_text_section(panel, ctx, "Passif", cyan, cyan.with_alpha(0.10), passive);
            }
            if let Some(active) = &view.ship_active {
                spawn_text_section(panel, ctx, "Actif", cyan, cyan.with_alpha(0.10), active);
            }

            panel
                .spawn((row(8.), Pickable::IGNORE))
                .with_children(|actions| {
                    if view.can_afford {
                        spawn_button(
                            actions,
                            ctx,
                            ButtonSpec::new(
                                "Confirmer",
                                ButtonTone::Gold,
                                view.confirm_command.clone(),
                            )
                            .tall()
                            .grow(),
                        );
                    } else {
                        spawn_button(
                            actions,
                            ctx,
                            ButtonSpec::new(
                                "Volonté insuffisante",
                                ButtonTone::Ghost,
                                UiCommand::Ignore,
                            )
                            .tall()
                            .grow()
                            .disabled(true),
                        );
                    }
                    spawn_button(
                        actions,
                        ctx,
                        ButtonSpec::new("Annuler", ButtonTone::Ghost, UiCommand::Reset)
                            .tall()
                            .grow(),
                    );
                });
        },
    );
    true
}

/// A titled box with one paragraph.
fn spawn_text_section(
    parent: &mut ChildSpawnerCommands,
    ctx: &PanelCtx,
    caption: &str,
    caption_color: Color,
    tint: Color,
    text: &str,
) {
    let caption = caption.to_string();
    let text = text.to_string();
    parent
        .spawn(section(tint, Some(caption_color.with_alpha(0.22))))
        .with_children(|node| {
            spawn_caption(node, ctx, caption, caption_color);
            node.spawn(body(
                text,
                &ctx.fonts.spectral,
                L::FS_BODY,
                ctx.palette.text.with_alpha(0.86),
            ));
        });
}

// ============================================================
// Counter window (Game.tsx::renderCounterWindow)
// ============================================================

const COUNTER_W: f32 = 470.0;

fn spawn_counter_window(
    commands: &mut Commands,
    ctx: &PanelCtx,
    session: &Session,
    z: i32,
) -> bool {
    let Some(view) = model::counter_view(&session.state, &session.registry, &session.valid) else {
        return false;
    };
    let palette = *ctx.palette;

    spawn_modal(
        commands,
        ctx,
        z,
        COUNTER_W,
        palette.target.with_alpha(0.65),
        "CounterWindow",
        Dismiss::Never,
        |panel| {
            panel
                .spawn((row(8.), Pickable::IGNORE))
                .with_children(|header| {
                    header.spawn((
                        Node {
                            width: px(9.),
                            height: px(9.),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        BackgroundColor(palette.target),
                        Pickable::IGNORE,
                    ));
                    header.spawn((
                        line(
                            "Attaque entrante",
                            &ctx.fonts.cinzel_bold,
                            L::FS_TITLE - 3.0,
                            palette.foe,
                        ),
                        bevy::text::LetterSpacing::Px(1.6),
                    ));
                });

            panel
                .spawn(section(Color::srgba(1., 1., 1., 0.04), None))
                .with_children(|info| {
                    info.spawn((
                        Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            align_items: AlignItems::Center,
                            column_gap: px(5.),
                            row_gap: px(2.),
                            ..default()
                        },
                        Pickable::IGNORE,
                    ))
                    .with_children(|sentence| {
                        sentence.spawn(line(
                            view.attacker.clone(),
                            &ctx.fonts.spectral_bold,
                            L::FS_BODY + 1.0,
                            palette.foe,
                        ));
                        sentence.spawn(line(
                            "attaque",
                            &ctx.fonts.spectral,
                            L::FS_BODY,
                            palette.text_dim,
                        ));
                        sentence.spawn(line(
                            view.target.clone(),
                            &ctx.fonts.spectral_bold,
                            L::FS_BODY + 1.0,
                            palette.def,
                        ));
                    });

                    info.spawn((
                        Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: px(8.),
                            margin: UiRect::top(px(4.)),
                            ..default()
                        },
                        Pickable::IGNORE,
                    ))
                    .with_children(|numbers| {
                        numbers.spawn(line(
                            view.damage.to_string(),
                            &ctx.fonts.bangers,
                            30.0,
                            palette.foe,
                        ));
                        numbers.spawn(line(
                            "dégâts",
                            &ctx.fonts.oswald,
                            L::FS_BODY,
                            palette.text_faint,
                        ));
                        if let Some(element) = view.element {
                            spawn_pill(
                                numbers,
                                &ctx.fonts.oswald,
                                element_label(element),
                                Color::WHITE,
                                element_color(element),
                                L::FS_LABEL,
                            );
                        }
                        if view.has_haki {
                            spawn_pill(
                                numbers,
                                &ctx.fonts.oswald,
                                "Haki",
                                Color::srgb(0.80, 0.70, 1.0),
                                Color::srgba(0.45, 0.25, 0.86, 0.45),
                                L::FS_LABEL,
                            );
                        }
                    });
                });

            panel
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: px(8.),
                        row_gap: px(8.),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|actions| {
                    for option in &view.options {
                        let (tone, glyph) = match option.kind {
                            CounterKind::PlayCounter => (ButtonTone::Blue, Some(SHIELD)),
                            CounterKind::Shield => (ButtonTone::Gold, Some(SHIELD)),
                            CounterKind::Haki => (ButtonTone::Purple, Some(EYE)),
                            CounterKind::Pass => (ButtonTone::Ghost, None),
                        };
                        let mut spec =
                            ButtonSpec::new(option.label.clone(), tone, option.command.clone());
                        if let Some(glyph) = glyph {
                            spec = spec.glyph(glyph);
                        }
                        spawn_button(actions, ctx, spec.tall());
                    }
                });
        },
    );
    true
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::testkit::{advance, session as make_session};
    use crate::selection::AttackKind;
    use model::{ActionMenuView, CounterView};

    #[test]
    fn idle_shows_nothing() {
        assert!(active_panels(&UiMode::Idle, false).is_empty());
        assert!(active_panels(&UiMode::SelectingCaptainSlot, false).is_empty());
        assert!(
            active_panels(
                &UiMode::SelectingTarget {
                    attacker_id: "a".into(),
                    kind: AttackKind::Base
                },
                false
            )
            .is_empty()
        );
    }

    #[test]
    fn every_modal_mode_maps_to_one_panel() {
        assert_eq!(
            active_panels(
                &UiMode::ActionMenu {
                    instance_id: "u1".into()
                },
                false
            ),
            vec![PanelKind::ActionMenu {
                instance_id: "u1".into()
            }]
        );
        assert_eq!(
            active_panels(
                &UiMode::CaptainMenu {
                    player_id: PlayerId::Player2
                },
                false
            ),
            vec![PanelKind::CaptainMenu {
                player: PlayerId::Player2
            }]
        );
        assert_eq!(
            active_panels(
                &UiMode::ShipMenu {
                    instance_id: "s1".into(),
                    is_you: true
                },
                false
            ),
            vec![PanelKind::ShipMenu {
                instance_id: "s1".into(),
                is_you: true
            }]
        );
        assert_eq!(
            active_panels(
                &UiMode::CardDetail {
                    def_id: "MG-001".into(),
                    instance_id: None
                },
                false
            ),
            vec![PanelKind::CardDetail {
                def_id: "MG-001".into(),
                instance_id: None
            }]
        );
        assert_eq!(
            active_panels(
                &UiMode::ConfirmEvent {
                    instance_id: "e1".into()
                },
                false
            ),
            vec![PanelKind::Confirm {
                instance_id: "e1".into(),
                is_ship: false
            }]
        );
        assert_eq!(
            active_panels(
                &UiMode::ConfirmShip {
                    instance_id: "e1".into()
                },
                false
            ),
            vec![PanelKind::Confirm {
                instance_id: "e1".into(),
                is_ship: true
            }]
        );
    }

    #[test]
    fn the_counter_window_is_drawn_last_so_it_wins() {
        let panels = active_panels(
            &UiMode::CardDetail {
                def_id: "MG-001".into(),
                instance_id: None,
            },
            true,
        );
        assert_eq!(panels.len(), 2);
        assert_eq!(panels.last(), Some(&PanelKind::Counter));
    }

    #[test]
    fn a_counter_window_alone_needs_no_mode() {
        assert_eq!(active_panels(&UiMode::Idle, true), vec![PanelKind::Counter]);
    }

    #[test]
    fn the_plugin_boots_headlessly_and_keeps_the_ui_idle() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<AppScreen>()
            .add_plugins((
                crate::bridge::BridgePlugin,
                crate::selection::SelectionPlugin,
                PanelsPlugin,
            ))
            .insert_resource(make_session(42));

        app.update();

        assert!(app.world().get_resource::<PanelCache>().is_some());
        assert_eq!(*app.world().resource::<UiMode>(), UiMode::Idle);
        let world = app.world_mut();
        let mut roots = world.query_filtered::<Entity, With<PanelRoot>>();
        assert_eq!(roots.iter(world).count(), 0, "no window, no panels");
    }

    #[test]
    fn a_real_counter_window_produces_a_view_with_dispatchable_buttons() {
        let mut session = make_session(13);
        let human = session.human;
        if !advance(&mut session, 900, |s| {
            s.state.pending_attack.is_some()
                && s.state.current_player != human
                && !s.valid.is_empty()
        }) {
            return;
        }
        assert_eq!(
            active_panels(&UiMode::Idle, session.in_counter_window()),
            vec![PanelKind::Counter]
        );
        let view: CounterView =
            model::counter_view(&session.state, &session.registry, &session.valid).unwrap();
        for option in &view.options {
            // `DispatchKeepUi`, not `Dispatch`: the TS counter buttons are the
            // only dispatch sites that do **not** call `resetUI()`.
            let UiCommand::DispatchKeepUi(action) = &option.command else {
                panic!("a counter button must dispatch without resetting the UI");
            };
            assert!(
                session.valid.contains(action),
                "every button comes from Session::valid"
            );
        }

        // Answering the attack must leave an open panel exactly where it was.
        let mut mode = UiMode::CardDetail {
            def_id: "MG-001".into(),
            instance_id: None,
        };
        let mut selected = SelectedHandCard(Some("h1".into()));
        let before = (mode.clone(), selected.clone());
        let out = apply_ui_command(view.options[0].command.clone(), &mut mode, &mut selected);
        assert!(out.is_some());
        assert_eq!((mode, selected), before);
    }

    /// The popover is placed on its tile, and flips below when the tile is too
    /// close to the top edge.
    #[test]
    fn the_popover_hangs_off_its_tile() {
        let window = Vec2::new(L::WINDOW_W, L::WINDOW_H);
        let size = Vec2::new(L::POPOVER_W, 120.0);

        let tile = Rect::from_center_size(Vec2::new(640.0, 500.0), Vec2::new(120.0, 60.0));
        let (at, above) = popover_placement(tile, size, window);
        assert!(above, "there is room above a mid-board tile");
        assert!((at.x + size.x * 0.5 - tile.center().x).abs() < 1e-3);
        assert!((at.y + size.y + L::POPOVER_GAP - tile.min.y).abs() < 1e-3);

        // A tile right under the header: the popover flips below it.
        let high = Rect::from_center_size(Vec2::new(640.0, 70.0), Vec2::new(120.0, 60.0));
        let (below, above) = popover_placement(high, size, window);
        assert!(!above);
        assert!(below.y > high.max.y);

        // …and it never leaves the window sideways.
        let edge = Rect::from_center_size(Vec2::new(10.0, 500.0), Vec2::new(120.0, 60.0));
        assert!(popover_placement(edge, size, window).0.x >= 0.0);
        let far = Rect::from_center_size(Vec2::new(1275.0, 500.0), Vec2::new(120.0, 60.0));
        assert!(popover_placement(far, size, window).0.x + size.x <= window.x);
    }

    #[test]
    fn the_action_menu_only_offers_actions_the_engine_allows() {
        let (session, id) =
            model::deploy_one(7).expect("seed 7 must let the human deploy a character");
        let view: ActionMenuView =
            model::action_menu_view(&session.state, &session.registry, &session.valid, &id)
                .unwrap();
        for option in [view.base.as_ref(), view.special.as_ref()]
            .into_iter()
            .flatten()
        {
            if let UiCommand::Dispatch(action) = &option.command {
                assert!(
                    session.valid.contains(action) || option.disabled,
                    "an enabled row never dispatches an illegal action"
                );
            }
        }
    }

    /// An open panel is respawned **only** when its own content changed.
    ///
    /// The layer used to bump a stamp on every `StateChanged`, so a card detail
    /// left open while the AI played was torn down and rebuilt once per AI
    /// action — losing its buttons' hover state each time.
    #[test]
    fn an_open_panel_survives_an_unrelated_state_change() {
        use crate::app::{AppScreen, Fonts, Palette};
        use crate::art::ArtCache;
        use crate::bridge::{BridgePlugin, DispatchAction};
        use crate::selection::SelectionPlugin;
        use tcgop_engine::types::GameAction;

        let session = make_session(7);
        let def_id = session
            .state
            .card(&session.you().hand[0])
            .unwrap()
            .def_id
            .clone();

        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(bevy::state::app::StatesPlugin)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .init_asset::<Image>()
            .init_state::<AppScreen>()
            .insert_resource(Palette::default())
            .insert_resource(Fonts::default())
            .init_resource::<ArtCache>()
            .add_plugins((BridgePlugin, SelectionPlugin, PanelsPlugin))
            .insert_resource(session);
        app.world_mut()
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::Board);
        app.update();

        *app.world_mut().resource_mut::<UiMode>() = UiMode::CardDetail {
            def_id,
            instance_id: None,
        };
        app.update();

        let roots = |app: &mut App| -> Vec<Entity> {
            let mut q = app.world_mut().query_filtered::<Entity, With<PanelRoot>>();
            q.iter(app.world()).collect()
        };
        let before = roots(&mut app);
        assert_eq!(before.len(), 1, "the detail is open");

        // The turn changes hands: a real `StateChanged`, but nothing this
        // panel draws moved.
        app.world_mut()
            .write_message(DispatchAction(GameAction::EndTurn));
        app.update();
        app.update();

        assert_eq!(
            roots(&mut app),
            before,
            "the panel must not be despawned and respawned"
        );
    }

    #[test]
    fn a_missing_instance_never_panics() {
        let session = make_session(1);
        assert!(
            model::action_menu_view(&session.state, &session.registry, &session.valid, "ghost")
                .is_none()
        );
        assert!(
            model::ship_menu_view(
                &session.state,
                &session.registry,
                &session.valid,
                "ghost",
                true
            )
            .is_none()
        );
        assert!(model::confirm_view(&session.state, &session.registry, "ghost", false).is_none());
    }
}

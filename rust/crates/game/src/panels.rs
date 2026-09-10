//! Every modal of the client, as Bevy UI overlays.
//!
//! One system rebuilds the whole overlay stack whenever the
//! [`UiMode`](crate::selection::UiMode) or the engine state changes:
//!
//! | mode / condition | web | built by |
//! |---|---|---|
//! | `ActionMenu` | `ActionMenu.tsx` | [`spawn_action_menu`] |
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
//! The amber "clique ton Capitaine / ton Navire" hint box lives in the action
//! column next to the hand and is built by
//! [`hand`](crate::hand) (`footer_hints`), not here.

pub mod model;
pub mod widgets;

use bevy::prelude::*;
use tcgop_engine::state::CardInstance;
use tcgop_engine::types::{CardDef, PlayerId};

use crate::app::{AppScreen, AppSet, Fonts, Palette, configure_pipeline, layout as L};
use crate::art::{ArtCache, faction_visual, trait_color, trait_label};
use crate::board::interaction::apply_ui_command;
use crate::bridge::{BridgeSet, DispatchAction, Session, StateChanged};
use crate::hand::SymbolFont;
use crate::hand::card_face::{CardFace, FaceCtx, element_color, element_label, spawn_card_face};
use crate::selection::{SelectedHandCard, UiCommand, UiMode};

use model::{
    AbilityKind, AbilityRow, AttackOption, CounterKind, StatusChip, haki_label, turns_label,
};
use widgets::{
    ANCHOR, BOLT, ButtonSpec, ButtonTone, CROWN, ClickCommand, EYE, PanelButton, PanelCtx,
    SHIELD, SPARKLE, STAR, SWORD, body, column, line, panel, row, scrim, section, spawn_button,
    spawn_caption, spawn_close_cross, spawn_gauge, spawn_pill,
};

/// `▸` — the "this row is clickable" caret of `FullCard.renderAction`.
const CARET: &str = "\u{25BA}";

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
            .init_resource::<PanelCache>()
            .add_observer(route_panel_clicks)
            .add_systems(
                Update,
                (rebuild_panels, highlight_panel_buttons)
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
        UiMode::CaptainMenu { player_id } => panels.push(PanelKind::CaptainMenu {
            player: *player_id,
        }),
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
    /// Bumped on every [`StateChanged`] so an open panel refreshes its numbers.
    stamp: u64,
    built_stamp: u64,
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
}

#[allow(clippy::too_many_arguments)]
fn rebuild_panels(
    mut commands: Commands,
    mut changed: MessageReader<StateChanged>,
    mut cache: ResMut<PanelCache>,
    session: Res<Session>,
    mode: Res<UiMode>,
    palette: Res<Palette>,
    fonts: Res<Fonts>,
    symbols: Res<SymbolFont>,
    assets: Res<AssetServer>,
    mut art: ResMut<ArtCache>,
    roots: Query<Entity, With<PanelRoot>>,
) {
    if changed.read().count() > 0 {
        cache.stamp = cache.stamp.wrapping_add(1);
    }

    let wanted = active_panels(&mode, session.in_counter_window());
    if cache.key.as_ref() == Some(&wanted) && cache.built_stamp == cache.stamp {
        return;
    }
    cache.key = Some(wanted.clone());
    cache.built_stamp = cache.stamp;

    for entity in &roots {
        commands.entity(entity).despawn();
    }

    let ctx = PanelCtx {
        palette: &palette,
        fonts: &fonts,
        symbols: &symbols.0,
    };
    let mut z = L::Z_MODAL;
    for kind in &wanted {
        match kind {
            PanelKind::ActionMenu { instance_id } => {
                spawn_action_menu(
                    &mut commands,
                    &ctx,
                    &session,
                    &assets,
                    &mut art,
                    z,
                    instance_id,
                );
            }
            PanelKind::CaptainMenu { player } => {
                spawn_captain_menu(&mut commands, &ctx, &session, z, *player);
            }
            PanelKind::ShipMenu {
                instance_id,
                is_you,
            } => {
                spawn_ship_menu(
                    &mut commands,
                    &ctx,
                    &session,
                    &assets,
                    &mut art,
                    z,
                    instance_id,
                    *is_you,
                );
            }
            PanelKind::CardDetail {
                def_id,
                instance_id,
            } => {
                spawn_card_detail(
                    &mut commands,
                    &ctx,
                    &session,
                    &assets,
                    &mut art,
                    z,
                    def_id,
                    instance_id.as_deref(),
                );
            }
            PanelKind::Confirm {
                instance_id,
                is_ship,
            } => {
                spawn_confirm(&mut commands, &ctx, &session, z, instance_id, *is_ship);
            }
            PanelKind::Counter => {
                spawn_counter_window(&mut commands, &ctx, &session, z);
            }
        }
        z += 4;
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
fn highlight_panel_buttons(
    buttons: Query<(&Interaction, &PanelButton, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, button, mut background) in buttons {
        background.0 = match interaction {
            Interaction::None => button.base,
            _ => button.hover,
        };
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
            .with_children(build);
    });
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
    parent
        .spawn(section(tint, None))
        .with_children(|entries| {
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

const ACTION_MENU_W: f32 = 552.0;
const ACTION_CARD_W: f32 = 232.0;

#[allow(clippy::too_many_arguments)]
fn spawn_action_menu(
    commands: &mut Commands,
    ctx: &PanelCtx,
    session: &Session,
    assets: &AssetServer,
    art: &mut ArtCache,
    z: i32,
    instance_id: &str,
) {
    let Some(view) = model::action_menu_view(
        &session.state,
        &session.registry,
        &session.valid,
        instance_id,
    ) else {
        return;
    };
    let Some(instance) = session.state.card(instance_id) else {
        return;
    };
    let Some(def) = session.registry.card_def(&instance.def_id) else {
        return;
    };
    let instance = instance.clone();
    let def = def.clone();
    let palette = *ctx.palette;

    spawn_modal(
        commands,
        ctx,
        z,
        ACTION_MENU_W,
        palette.gold.with_alpha(0.55),
        "ActionMenu",
        Dismiss::OnBackdrop,
        |panel| {
            // Header: hint on the left, Volonté on the right.
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
                    spawn_caption(header, ctx, "Choisissez une action", palette.text_faint);
                    header.spawn(line(
                        format!("{} Vol.", view.volonte),
                        &ctx.fonts.oswald_bold,
                        L::FS_NAME,
                        palette.gold,
                    ));
                });

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
                            instance: Some(&instance),
                            atk: view.atk,
                            def_value: view.def,
                            width: ACTION_CARD_W,
                        },
                    );

                    split
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: px(8.),
                                flex_grow: 1.,
                                min_width: px(0.),
                                ..default()
                            },
                            Pickable::IGNORE,
                        ))
                        .with_children(|col| {
                            col.spawn(line(
                                view.name.clone(),
                                &ctx.fonts.cinzel_bold,
                                L::FS_NAME + 2.0,
                                palette.text,
                            ));
                            spawn_stat_pair(col, ctx, view.atk, view.def);
                            spawn_flag_pills(
                                col,
                                ctx,
                                &view.flags,
                                palette.amber,
                                palette.amber.with_alpha(0.18),
                            );

                            if let Some(base) = &view.base {
                                spawn_attack_row(col, ctx, base);
                            }
                            if let Some(special) = &view.special {
                                spawn_attack_row(col, ctx, special);
                            }

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
                        });
                });

            panel
                .spawn((row(8.), Pickable::IGNORE))
                .with_children(|actions| {
                    spawn_button(
                        actions,
                        ctx,
                        ButtonSpec::new("Détails", ButtonTone::Ghost, view.detail_command.clone())
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
}

/// One clickable rules row — TS `FullCard.renderAction` with `actions.base` /
/// `actions.special` wired up.
fn spawn_attack_row(parent: &mut ChildSpawnerCommands, ctx: &PanelCtx, option: &AttackOption) {
    let palette = ctx.palette;
    let accent = if option.is_special {
        palette.atk
    } else {
        palette.deploy
    };
    let alpha = if option.disabled { 0.5 } else { 1.0 };
    let fill = Color::srgba(1., 1., 1., 0.05);
    let glyph = if option.is_special {
        STAR
    } else if option.atk.is_none() {
        SPARKLE
    } else {
        SWORD
    };

    let mut node = parent.spawn((
        Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Start,
            column_gap: px(6.),
            padding: UiRect::axes(px(8.), px(6.)),
            border: UiRect::all(px(1.)),
            border_radius: BorderRadius::all(px(9.)),
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(accent.with_alpha(if option.disabled { 0.12 } else { 0.42 })),
    ));
    if !option.disabled {
        node.insert((
            ClickCommand(option.command.clone()),
            Button,
            PanelButton {
                base: fill,
                hover: accent.with_alpha(0.16),
            },
        ));
    }

    let option = option.clone();
    node.with_children(|row_node| {
        row_node
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(1.),
                    flex_grow: 1.,
                    flex_shrink: 1.,
                    min_width: px(0.),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|left| {
                left.spawn((
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
                .with_children(|head| {
                    head.spawn(line(
                        glyph,
                        ctx.symbols,
                        L::FS_STAT,
                        accent.with_alpha(alpha),
                    ));
                    head.spawn(line(
                        option.name.clone(),
                        &ctx.fonts.spectral_bold,
                        L::FS_BODY + 0.5,
                        palette.text.with_alpha(alpha),
                    ));
                    if let Some(element) = option.element {
                        spawn_pill(
                            head,
                            &ctx.fonts.oswald,
                            element_label(element),
                            Color::WHITE.with_alpha(alpha),
                            element_color(element).with_alpha(alpha),
                            L::FS_TINY + 0.5,
                        );
                    }
                    for attack_trait in &option.attack_traits {
                        spawn_pill(
                            head,
                            &ctx.fonts.oswald,
                            crate::hand::card_face::attack_trait_label(*attack_trait),
                            Color::WHITE.with_alpha(alpha),
                            Color::srgba(1., 1., 1., 0.22 * alpha),
                            L::FS_TINY + 0.5,
                        );
                    }
                    if !option.disabled {
                        head.spawn(line(CARET, ctx.symbols, L::FS_LABEL, accent));
                    }
                });
                if let Some(description) = &option.description {
                    left.spawn(body(
                        description.clone(),
                        &ctx.fonts.spectral,
                        L::FS_LABEL,
                        palette.text_dim.with_alpha(alpha * 0.78),
                    ));
                }
                if option.disabled
                    && let Some(reason) = &option.reason
                {
                    left.spawn(line(
                        format!("\u{2022} {reason}"),
                        &ctx.fonts.oswald,
                        L::FS_LABEL - 1.0,
                        Color::srgb(1.0, 0.54, 0.50),
                    ));
                }
            });

        row_node
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::End,
                    flex_shrink: 0.,
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|right| {
                right.spawn(line(
                    format!("{} Vol.", option.cost),
                    &ctx.fonts.oswald,
                    L::FS_LABEL,
                    palette.text.with_alpha(0.92 * alpha),
                ));
                if let Some(atk) = option.atk {
                    right.spawn(line(
                        format!("ATK {atk}"),
                        &ctx.fonts.oswald_bold,
                        L::FS_LABEL,
                        palette.text.with_alpha(0.92 * alpha),
                    ));
                }
            });
    });
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
) {
    let Some(view) = model::captain_menu_view(
        &session.state,
        &session.registry,
        &session.valid,
        player,
        session.human,
    ) else {
        return;
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
                            spawn_button(
                                actions,
                                ctx,
                                ButtonSpec::new(
                                    "Engager le Capitaine",
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
) {
    let Some(view) = model::ship_menu_view(
        &session.state,
        &session.registry,
        &session.valid,
        instance_id,
        is_you,
    ) else {
        return;
    };
    let Some(instance) = session.state.card(instance_id).cloned() else {
        return;
    };
    let Some(def) = session.registry.card_def(&instance.def_id).cloned() else {
        return;
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

            spawn_button(
                panel,
                ctx,
                ButtonSpec::new("Fermer", ButtonTone::Ghost, UiCommand::Reset),
            );
        },
    );
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
) {
    let Some(view) = model::card_detail_view(&session.state, &session.registry, def_id, instance_id)
    else {
        return;
    };
    let Some(def) = session.registry.card_def(def_id).cloned() else {
        return;
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
) {
    let Some(view) =
        model::confirm_view(&session.state, &session.registry, instance_id, is_ship)
    else {
        return;
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

fn spawn_counter_window(commands: &mut Commands, ctx: &PanelCtx, session: &Session, z: i32) {
    let Some(view) = model::counter_view(&session.state, &session.registry, &session.valid) else {
        return;
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
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::testkit::{advance, session as make_session};
    use model::{ActionMenuView, CounterView};

    #[test]
    fn idle_shows_nothing() {
        assert!(active_panels(&UiMode::Idle, false).is_empty());
        assert!(active_panels(&UiMode::SelectingCaptainSlot, false).is_empty());
        assert!(
            active_panels(
                &UiMode::SelectingTarget {
                    attacker_id: "a".into(),
                    is_special: false
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
            s.state.pending_attack.is_some() && s.state.current_player != human && !s.valid.is_empty()
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
            let UiCommand::Dispatch(action) = &option.command else {
                panic!("a counter button must dispatch");
            };
            assert!(
                session.valid.contains(action),
                "every button comes from Session::valid"
            );
        }
    }

    #[test]
    fn the_action_menu_only_offers_actions_the_engine_allows() {
        let (session, id) =
            model::deploy_one(7).expect("seed 7 must let the human deploy a character");
        let view: ActionMenuView = model::action_menu_view(
            &session.state,
            &session.registry,
            &session.valid,
            &id,
        )
        .unwrap();
        for option in [view.base.as_ref(), view.special.as_ref()].into_iter().flatten() {
            if let UiCommand::Dispatch(action) = &option.command {
                assert!(
                    session.valid.contains(action) || option.disabled,
                    "an enabled row never dispatches an illegal action"
                );
            }
        }
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

//! The painters: every piece of board content, spawned from a [`view`](crate::board::model)
//! value.
//!
//! Each function fills a **content root** whose children are despawned and
//! respawned when — and only when — the value it was built from changed, so
//! unchanged tiles never flicker.

use bevy::prelude::*;
use tcgop_engine::types::Slot;

use crate::app::{Fonts, Palette, layout as l};
use crate::art::{ArtCache, Crest, Focus};
use crate::board::geometry::HalfMetrics;
use crate::board::model::{
    CaptainCardView, CaptainTokenView, CellContent, CellView, HeaderView, ResourceView, ShipView,
    UnitView,
};
use crate::board::style::{caption, overlay, text, tone_color};

/// Marker on an [`ImageNode`] that must be cropped "cover"-style around a focal
/// point once its texture size is known (see `apply_cover_fit`).
#[derive(Component, Debug, Clone, Copy)]
pub struct CoverFit {
    pub focus: Focus,
}

/// Everything a painter needs, gathered once per sync.
pub struct Painter<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub palette: &'a Palette,
    pub fonts: &'a Fonts,
    pub art: &'a mut ArtCache,
    pub assets: &'a AssetServer,
    pub metrics: HalfMetrics,
}

/// An absolutely positioned node covering its parent.
pub fn fill_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(0.0),
        right: px(0.0),
        top: px(0.0),
        bottom: px(0.0),
        ..default()
    }
}

fn row_word(slot: Slot) -> &'static str {
    match slot {
        Slot::V1 | Slot::V2 | Slot::V3 => "Avant",
        Slot::A1 | Slot::A2 | Slot::A3 => "Arrière",
    }
}

fn slot_code(slot: Slot) -> &'static str {
    match slot {
        Slot::V1 => "V1",
        Slot::V2 => "V2",
        Slot::V3 => "V3",
        Slot::A1 => "A1",
        Slot::A2 => "A2",
        Slot::A3 => "A3",
    }
}

impl Painter<'_, '_, '_> {
    // --------------------------------------------------------
    // Primitives
    // --------------------------------------------------------

    fn child(&mut self, parent: Entity, bundle: impl Bundle) -> Entity {
        self.commands.spawn((bundle, ChildOf(parent))).id()
    }

    /// A cropped illustration filling `parent`.
    pub fn art(&mut self, parent: Entity, path: &str, focus: Focus, flip_y: bool) -> Entity {
        let handle = self.art.image(self.assets, path);
        self.child(
            parent,
            (
                ImageNode {
                    image: handle,
                    image_mode: NodeImageMode::Stretch,
                    flip_y,
                    ..default()
                },
                fill_node(),
                CoverFit { focus },
                Pickable::IGNORE,
            ),
        )
    }

    /// The dark-to-transparent wash that keeps text readable over an
    /// illustration (`.sh` / `.shade` of the mock-up).
    pub fn shade(&mut self, parent: Entity, top: f32, bottom: f32) -> Entity {
        let ink = self.palette.bg_deep;
        self.child(
            parent,
            (
                fill_node(),
                BackgroundGradient::from(LinearGradient::to_bottom(vec![
                    ColorStop::new(ink.with_alpha(top), percent(0.0)),
                    ColorStop::new(ink.with_alpha(bottom), percent(100.0)),
                ])),
                Pickable::IGNORE,
            ),
        )
    }

    /// A rounded gauge: dark trough + coloured fill.
    pub fn gauge(&mut self, parent: Entity, height: f32, ratio: f32, color: Color) -> Entity {
        let bar = self.child(
            parent,
            (
                Node {
                    flex_grow: 1.0,
                    height: px(height),
                    border_radius: BorderRadius::MAX,
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(Color::BLACK.with_alpha(0.55)),
                Pickable::IGNORE,
            ),
        );
        self.child(
            bar,
            (
                Node {
                    width: percent((ratio.clamp(0.0, 1.0) * 100.0).max(0.0)),
                    height: percent(100.0),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(color),
                Pickable::IGNORE,
            ),
        );
        bar
    }

    /// A small rounded pill of text (counts, chips…).
    pub fn pill(
        &mut self,
        parent: Entity,
        value: impl Into<String>,
        size: f32,
        color: Color,
        background: Color,
    ) -> Entity {
        let pill = self.child(
            parent,
            (
                Node {
                    padding: UiRect::axes(px(5.0), px(1.0)),
                    border_radius: BorderRadius::all(px(6.0)),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(background),
                Pickable::IGNORE,
            ),
        );
        let font = self.fonts.oswald_bold.clone();
        self.child(pill, text(value, &font, size, color));
        pill
    }

    // --------------------------------------------------------
    // Cells
    // --------------------------------------------------------

    /// Fill a cell's content root.
    pub fn cell_content(&mut self, root: Entity, cell: &CellView) {
        match &cell.content {
            CellContent::Empty => self.empty_cell(root, cell.slot),
            CellContent::Unit(unit) => self.unit_tile(root, unit),
            CellContent::Captain(token) => self.captain_token(root, token),
        }
    }

    fn empty_cell(&mut self, root: Entity, slot: Slot) {
        let column = self.child(
            root,
            (
                Node {
                    width: percent(100.0),
                    height: percent(100.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: px(2.0),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        let oswald = self.fonts.oswald_bold.clone();
        let faint = self.palette.text_faint;
        let dim = self.palette.text_dim;
        self.child(column, text(slot_code(slot), &oswald, l::FS_LABEL, dim));
        self.child(
            column,
            caption(row_word(slot), &oswald, l::FS_TINY, faint, 1.6),
        );
    }

    fn unit_tile(&mut self, root: Entity, unit: &UnitView) {
        match unit.art {
            Some(path) => {
                self.art(root, path, unit.focus, false);
            }
            None => {
                let accent = self.palette.bg_slot;
                self.child(root, (fill_node(), BackgroundColor(accent), Pickable::IGNORE));
                let font = self.fonts.cinzel_bold.clone();
                let color = self.palette.text_dim;
                let box_ = self.child(
                    root,
                    (
                        Node {
                            width: percent(100.0),
                            height: percent(100.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        Pickable::IGNORE,
                    ),
                );
                self.child(box_, text(unit.name.clone(), &font, l::FS_NAME, color));
            }
        }

        // Status badges + equipment count, top-right.
        if !unit.badges.is_empty() || unit.equipment > 0 {
            let stack = self.child(
                root,
                (
                    Node {
                        position_type: PositionType::Absolute,
                        top: px(l::BADGE_GAP),
                        right: px(l::BADGE_GAP),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::End,
                        row_gap: px(l::BADGE_GAP),
                        ..default()
                    },
                    Pickable::IGNORE,
                ),
            );
            for badge in &unit.badges {
                let dot = self.child(
                    stack,
                    (
                        Node {
                            width: px(l::BADGE_D),
                            height: px(l::BADGE_D),
                            border: UiRect::all(px(1.0)),
                            border_radius: BorderRadius::MAX,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(badge.color.with_alpha(0.35)),
                        BorderColor::all(badge.color),
                        Pickable::IGNORE,
                    ),
                );
                if badge.turns >= 0 {
                    let font = self.fonts.oswald_bold.clone();
                    self.child(dot, text(badge.turns.to_string(), &font, l::FS_TINY, badge.color));
                }
            }
            if unit.equipment > 0 {
                let gold = self.palette.gold;
                self.pill(
                    stack,
                    format!("+{}", unit.equipment),
                    l::FS_TINY,
                    gold,
                    gold.with_alpha(0.25),
                );
            }
        }

        // Damaged → the thin HP bar, and nothing else.
        if unit.damaged {
            let color = self.palette.hp_color(unit.hp_ratio);
            let bar_row = self.child(
                root,
                (
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(l::SLOT_HP_BAR_INSET),
                        right: px(l::SLOT_HP_BAR_INSET),
                        bottom: px(4.0),
                        flex_direction: FlexDirection::Row,
                        ..default()
                    },
                    Pickable::IGNORE,
                ),
            );
            self.gauge(bar_row, l::SLOT_HP_BAR_H, unit.hp_ratio, color);
        }

        // Tapped veil.
        if unit.tapped {
            let veil = self.child(
                root,
                (
                    Node {
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..fill_node()
                    },
                    BackgroundColor(self.palette.bg_deep.with_alpha(0.55)),
                    Pickable::IGNORE,
                ),
            );
            let font = self.fonts.oswald_bold.clone();
            let color = self.palette.text;
            self.child(veil, caption("INCLINÉ", &font, l::FS_TINY, color, 2.0));
        }
    }

    fn captain_token(&mut self, root: Entity, token: &CaptainTokenView) {
        if let Some(path) = token.art {
            self.art(root, path, token.focus, false);
        }
        self.shade(root, 0.1, 0.9);

        let foe = self.palette.foe;
        let oswald = self.fonts.oswald_bold.clone();
        let badge = self.child(
            root,
            (
                Node {
                    position_type: PositionType::Absolute,
                    top: px(2.0),
                    left: px(4.0),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        self.child(badge, caption("VERSO", &oswald, l::FS_TINY, foe, 2.0));

        let bottom = self.child(
            root,
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: px(5.0),
                    right: px(5.0),
                    bottom: px(4.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(3.0),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        let cinzel = self.fonts.cinzel_bold.clone();
        let white = self.palette.text;
        self.child(bottom, text(token.name.clone(), &cinzel, l::FS_LABEL, white));
        let color = self.palette.hp_color(token.hp_ratio);
        let row = self.child(
            bottom,
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        self.gauge(row, l::SLOT_HP_BAR_H, token.hp_ratio, color);
    }

    // --------------------------------------------------------
    // Command bar
    // --------------------------------------------------------

    pub fn captain_content(&mut self, root: Entity, captain: &CaptainCardView) {
        if let Some(path) = captain.art {
            self.art(root, path, captain.focus, false);
        }
        self.shade(root, 0.15, 0.92);

        // ATK / DEF chips, top-right.
        let chips = self.child(
            root,
            (
                Node {
                    position_type: PositionType::Absolute,
                    top: px(4.0),
                    right: px(5.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::End,
                    row_gap: px(2.0),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        let ink = self.palette.bg_deep.with_alpha(0.7);
        let atk = self.palette.atk;
        let def = self.palette.def;
        self.pill(chips, format!("ATK {}", captain.atk), l::FS_TINY, atk, ink);
        self.pill(chips, format!("DEF {}", captain.def), l::FS_TINY, def, ink);

        // Role / name / PV, bottom-left.
        let info = self.child(
            root,
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: px(7.0),
                    right: px(7.0),
                    bottom: px(5.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2.0),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        let oswald = self.fonts.oswald.clone();
        let cinzel = self.fonts.cinzel_bold.clone();
        let dim = self.palette.text_dim;
        let white = self.palette.text;
        let role = if captain.flipped {
            "CAPITAINE ★ VERSO"
        } else {
            "CAPITAINE"
        };
        self.child(info, caption(role, &oswald, l::FS_TINY, dim, 2.0));
        self.child(info, text(captain.name.clone(), &cinzel, l::FS_NAME, white));

        let hp_row = self.child(
            info,
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(5.0),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        let color = self.palette.hp_color(captain.hp_ratio);
        self.gauge(hp_row, 7.0, captain.hp_ratio, color);
        let oswald_bold = self.fonts.oswald_bold.clone();
        self.child(
            hp_row,
            text(captain.pv.to_string(), &oswald_bold, l::FS_STAT, color),
        );

        if captain.tapped {
            let veil = self.palette.bg_deep.with_alpha(0.45);
            self.child(root, overlay(veil));
        }
    }

    pub fn ship_content(&mut self, root: Entity, ship: &ShipView) {
        let cyan = self.palette.def;
        match (&ship.name, ship.art) {
            (Some(name), art) => {
                if let Some(path) = art {
                    self.art(root, path, Focus::CENTER, false);
                }
                let label = self.child(
                    root,
                    (
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(0.0),
                            right: px(0.0),
                            bottom: px(0.0),
                            padding: UiRect::all(px(1.0)),
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(self.palette.bg_deep.with_alpha(0.8)),
                        Pickable::IGNORE,
                    ),
                );
                let font = self.fonts.oswald.clone();
                self.child(label, text(name.clone(), &font, l::FS_TINY, cyan));
            }
            (None, _) => {
                let box_ = self.child(
                    root,
                    (
                        Node {
                            width: percent(100.0),
                            height: percent(100.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            row_gap: px(2.0),
                            ..default()
                        },
                        Pickable::IGNORE,
                    ),
                );
                let font = self.fonts.oswald_bold.clone();
                let faded = cyan.with_alpha(0.6);
                self.child(box_, caption("NAVIRE", &font, l::FS_TINY, faded, 2.0));
            }
        }
    }

    pub fn resources_content(&mut self, root: Entity, res: &ResourceView) {
        let align = if res.is_you {
            AlignItems::Start
        } else {
            AlignItems::End
        };
        let column = self.child(
            root,
            (
                Node {
                    width: percent(100.0),
                    height: percent(100.0),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: align,
                    row_gap: px(5.0),
                    padding: UiRect::left(px(2.0)),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );

        if res.is_you {
            self.will_row(column, res);
            self.counts_row(column, res);
        } else {
            self.counts_row(column, res);
            self.will_row(column, res);
        }
    }

    fn will_row(&mut self, parent: Entity, res: &ResourceView) {
        let row = self.child(
            parent,
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(6.0),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        let oswald = self.fonts.oswald.clone();
        let cinzel = self.fonts.cinzel_bold.clone();
        let dim = self.palette.text_dim;
        let gold = self.palette.gold;
        self.child(row, caption("VOLONTÉ", &oswald, l::FS_TINY, dim, 1.8));

        // Pips only on your side (the mock-up gives the foe a bare number).
        if res.is_you {
            let pips = self.child(
                row,
                (
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: px(l::WILL_PIP_GAP),
                        ..default()
                    },
                    Pickable::IGNORE,
                ),
            );
            let off = self.palette.text.with_alpha(0.14);
            for i in 0..l::WILL_PIPS {
                let on = i < res.pips;
                self.child(
                    pips,
                    (
                        Node {
                            width: px(l::WILL_PIP_D),
                            height: px(l::WILL_PIP_D),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        BackgroundColor(if on { gold } else { off }),
                        Pickable::IGNORE,
                    ),
                );
            }
        }
        self.child(
            row,
            text(res.volonte.to_string(), &cinzel, l::FS_NAME, gold),
        );
    }

    fn counts_row(&mut self, parent: Entity, res: &ResourceView) {
        let row = self.child(
            parent,
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: px(6.0),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        let dim = self.palette.text_dim;
        let wash = self.palette.text.with_alpha(0.07);
        self.pill(row, format!("MAIN {}", res.hand), l::FS_LABEL, dim, wash);
        self.pill(row, format!("DECK {}", res.deck), l::FS_LABEL, dim, wash);
    }

    // --------------------------------------------------------
    // Header
    // --------------------------------------------------------

    pub fn header_content(&mut self, root: Entity, header: &HeaderView) {
        let gold = self.palette.gold;
        let cinzel = self.fonts.cinzel_bold.clone();
        let oswald = self.fonts.oswald.clone();
        let oswald_bold = self.fonts.oswald_bold.clone();
        let dim = self.palette.text_dim;
        let faint = self.palette.text_faint;

        self.child(root, text("TCGOP", &cinzel, l::FS_TITLE, gold));

        // Turn number, as a ring.
        let ring = self.child(
            root,
            (
                Node {
                    width: px(l::TURN_RING_D),
                    height: px(l::TURN_RING_D),
                    border: UiRect::all(px(2.0)),
                    border_radius: BorderRadius::MAX,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BorderColor::all(gold),
                BackgroundColor(gold.with_alpha(0.08)),
                Pickable::IGNORE,
            ),
        );
        self.child(
            ring,
            text(header.turn.to_string(), &cinzel, l::FS_STAT, gold),
        );
        self.child(root, caption("TOUR", &oswald, l::FS_TINY, faint, 3.0));

        // Status tag.
        let tone = tone_color(self.palette, header.status.tone);
        let tag = self.child(
            root,
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(6.0),
                    padding: UiRect::axes(px(10.0), px(4.0)),
                    margin: UiRect::left(px(12.0)),
                    border: UiRect::all(px(1.0)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(tone.with_alpha(0.14)),
                BorderColor::all(tone.with_alpha(0.5)),
                Pickable::IGNORE,
            ),
        );
        self.child(
            tag,
            (
                Node {
                    width: px(6.0),
                    height: px(6.0),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(tone),
                Pickable::IGNORE,
            ),
        );
        self.child(tag, text(header.status.text, &oswald_bold, l::FS_LABEL, tone));

        // Spacer, then the foe's crest and counts.
        self.child(
            root,
            (
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        self.crest(root, header.foe_crest, header.foe_accent);
        self.child(
            root,
            text(
                format!("MAIN {} · DECK {}", header.foe_hand, header.foe_deck),
                &oswald,
                l::FS_LABEL,
                dim,
            ),
        );
        if let Some(ship) = &header.foe_ship {
            let cyan = self.palette.def;
            let wash = cyan.with_alpha(0.16);
            self.pill(root, ship.clone(), l::FS_TINY, cyan, wash);
        }
    }

    /// The faction crest, drawn with plain nodes: a ringed disc for the straw
    /// hat, a barred disc for the marine anchor.
    fn crest(&mut self, parent: Entity, crest: Crest, accent: Color) -> Entity {
        let disc = self.child(
            parent,
            (
                Node {
                    width: px(14.0),
                    height: px(14.0),
                    border: UiRect::all(px(2.0)),
                    border_radius: BorderRadius::MAX,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BorderColor::all(accent),
                BackgroundColor(accent.with_alpha(0.18)),
                Pickable::IGNORE,
            ),
        );
        let (w, h) = match crest {
            Crest::Hat => (8.0, 2.0),
            Crest::Anchor => (2.0, 8.0),
        };
        self.child(
            disc,
            (
                Node {
                    width: px(w),
                    height: px(h),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(accent),
                Pickable::IGNORE,
            ),
        );
        disc
    }
}

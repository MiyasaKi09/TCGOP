//! The painters: every piece of board content, spawned from a [`view`](crate::board::model)
//! value.
//!
//! Each function fills a **content root** whose children are despawned and
//! respawned when — and only when — the value it was built from changed, so
//! unchanged tiles never flicker.

use bevy::prelude::*;
use tcgop_engine::types::Slot;

use crate::app::{Fonts, Palette, layout as l};
use crate::art::{ArtCache, Focus};
use crate::board::geometry::HalfMetrics;
use crate::board::model::{
    CaptainCardView, CaptainTokenView, CellContent, CellView, HeaderView, ResourceView, ShipView,
    UnitView,
};
use crate::board::style::{caption, overlay, text, tone_color};
use crate::hand::card_face::{ANCHOR, CARD_BACK, CROSSED_SWORDS, HAND_GLYPH, SHIELD};

/// Nominal pixel size of every illustration in `assets/cards` (they are all
/// 700x1050) and of the two ship-deck floors in `assets/decks`.
///
/// The real size is only known once the texture is loaded, and `ComputedNode`
/// is only known one frame after a node is spawned; cropping from these at
/// spawn time is what keeps a freshly repainted tile from rendering one
/// stretched frame before `apply_cover_fit` catches up.
pub const NOMINAL_CARD: Vec2 = Vec2::new(700.0, 1050.0);
/// `assets/decks/{mugiwara,marine}-deck-v.png`.
pub const NOMINAL_DECK: Vec2 = Vec2::new(941.0, 1672.0);

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
    /// The one font in `assets/fonts` with symbol coverage — the captain's
    /// ⚔ / 🛡 chips and the empty ship slot's ⚓ come from it.
    pub symbols: &'a Handle<Font>,
    pub art: &'a mut ArtCache,
    pub assets: &'a AssetServer,
    pub metrics: HalfMetrics,
}

/// Linear blend of two colours in sRGB — one stop of a gradient the UI has to
/// paint by hand (gradient-filled *text* has no Bevy equivalent).
fn mix(from: Color, to: Color, t: f32) -> Color {
    let (a, b) = (from.to_srgba(), to.to_srgba());
    let t = t.clamp(0.0, 1.0);
    Color::srgba(
        a.red + (b.red - a.red) * t,
        a.green + (b.green - a.green) * t,
        a.blue + (b.blue - a.blue) * t,
        a.alpha + (b.alpha - a.alpha) * t,
    )
}

/// Multiply a colour's channels, keeping its alpha — the second stop of the
/// mock-up's solid pills (`linear-gradient(90deg,#5fe39a,#37c97a)`).
fn darken(color: Color, factor: f32) -> Color {
    let c = color.to_srgba();
    Color::srgba(c.red * factor, c.green * factor, c.blue * factor, c.alpha)
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

/// The placeholder label of an empty slot.
///
/// Mock-up: the front line reads `V1..V3` and the back line `B1..B3`
/// ("**B**ack"), even though the engine's back-line slots are named `A1..A3`
/// ("**A**rrière"). The letter on screen follows the mock-up.
fn slot_code(slot: Slot) -> &'static str {
    match slot {
        Slot::V1 => "V1",
        Slot::V2 => "V2",
        Slot::V3 => "V3",
        Slot::A1 => "B1",
        Slot::A2 => "B2",
        Slot::A3 => "B3",
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
    ///
    /// `box_size` is the box it is cropped into and `nominal` the size the
    /// source image is expected to have: together they give the node its
    /// **final** rectangle on the frame it is spawned, so a repaint never
    /// flashes a stretched picture while `apply_cover_fit` waits for the
    /// computed layout. `apply_cover_fit` then refines it from the real
    /// texture, which for the shipped assets is the same rectangle.
    pub fn art(
        &mut self,
        parent: Entity,
        path: &str,
        focus: Focus,
        flip_y: bool,
        box_size: Vec2,
        nominal: Vec2,
    ) -> Entity {
        let handle = self.art.image(self.assets, path);
        let rect =
            crate::board::geometry::cover(box_size.x, box_size.y, nominal.x, nominal.y, focus);
        self.child(
            parent,
            (
                ImageNode {
                    image: handle,
                    image_mode: NodeImageMode::Stretch,
                    flip_y,
                    ..default()
                },
                Node {
                    position_type: PositionType::Absolute,
                    left: px(rect.left),
                    top: px(rect.top),
                    width: px(rect.width),
                    height: px(rect.height),
                    ..default()
                },
                CoverFit { focus },
                Pickable::IGNORE,
            ),
        )
    }

    /// The box one board tile is drawn in.
    fn tile_box(&self) -> Vec2 {
        Vec2::new(self.metrics.slot_w, self.metrics.slot_h)
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

    /// A rounded pill of "<glyph> <value>" — mock-up `.counts span`.
    ///
    /// The glyph comes from the symbol face and the number from Poppins, so a
    /// family without symbol coverage never swallows the icon.
    fn glyph_pill(
        &mut self,
        parent: Entity,
        glyph: &'static str,
        value: usize,
        size: f32,
        color: Color,
        background: Color,
    ) -> Entity {
        let pill = self.child(
            parent,
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(4.0),
                    padding: UiRect::axes(px(7.0), px(2.0)),
                    border_radius: BorderRadius::all(px(7.0)),
                    ..default()
                },
                BackgroundColor(background),
                Pickable::IGNORE,
            ),
        );
        let symbols = self.symbols.clone();
        let font = self.fonts.poppins_semi.clone();
        self.child(pill, text(glyph, &symbols, size, color));
        self.child(pill, text(value.to_string(), &font, size, color));
        pill
    }

    /// A compact glyph chip — mock-up `.capcard .ad span` (`⚔9` / `🛡4`).
    fn glyph_chip(
        &mut self,
        parent: Entity,
        glyph: &'static str,
        value: i32,
        size: f32,
        color: Color,
        background: Color,
    ) -> Entity {
        let chip = self.child(
            parent,
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(2.0),
                    padding: UiRect::axes(px(5.0), px(0.0)),
                    border_radius: BorderRadius::all(px(6.0)),
                    ..default()
                },
                BackgroundColor(background),
                Pickable::IGNORE,
            ),
        );
        let symbols = self.symbols.clone();
        let font = self.fonts.poppins_bold.clone();
        self.child(chip, text(glyph, &symbols, size, color));
        self.child(chip, text(value.to_string(), &font, size, color));
        chip
    }

    /// The mock-up's `border-style: dashed` placeholder outline, which Bevy UI
    /// has no equivalent for: four rows of evenly spaced ticks along the edges.
    ///
    /// Every tick is a **percentage** of its edge, never a fixed 6 px: a fixed
    /// tick needs `ticks * 6 px` of room and silently welds itself into a solid
    /// line as soon as the edge is shorter than that — which the 62 px-wide
    /// empty ship slot was. A tick of `100 / (2 * n - 1)` percent leaves a gap
    /// exactly as wide as a tick, at every size.
    fn dashed_outline(&mut self, parent: Entity, color: Color, thickness: f32) {
        let dash = thickness.max(1.0);
        for (horizontal, near) in [(true, true), (true, false), (false, true), (false, false)] {
            let mut node = Node {
                position_type: PositionType::Absolute,
                flex_direction: if horizontal {
                    FlexDirection::Row
                } else {
                    FlexDirection::Column
                },
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            };
            let inset = self.metrics.chrome(8.0);
            if horizontal {
                node.left = px(inset);
                node.right = px(inset);
                node.height = px(dash);
                if near {
                    node.top = px(0.0)
                } else {
                    node.bottom = px(0.0)
                }
            } else {
                node.top = px(inset);
                node.bottom = px(inset);
                node.width = px(dash);
                if near {
                    node.left = px(0.0)
                } else {
                    node.right = px(0.0)
                }
            }
            let edge = self.child(parent, (node, Pickable::IGNORE));
            let ticks: usize = if horizontal { 7 } else { 3 };
            let span = percent(100.0 / (2 * ticks - 1) as f32);
            for _ in 0..ticks {
                let tick = if horizontal {
                    Node {
                        width: span,
                        height: px(dash),
                        flex_shrink: 0.0,
                        ..default()
                    }
                } else {
                    Node {
                        width: px(dash),
                        height: span,
                        flex_shrink: 0.0,
                        ..default()
                    }
                };
                self.child(edge, (tick, BackgroundColor(color), Pickable::IGNORE));
            }
        }
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

    /// Mock-up `.slot.empty` — a dashed placeholder carrying **one** label
    /// (`V1`, `A2`), never a second caption line.
    fn empty_cell(&mut self, root: Entity, slot: Slot) {
        let dash = self.palette.text.with_alpha(0.20);
        self.dashed_outline(root, dash, self.metrics.chrome(1.5));

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
        let font = self.fonts.poppins_semi.clone();
        let faint = self.palette.text_faint;
        self.child(
            box_,
            caption(slot_code(slot), &font, l::FS_LABEL, faint, 1.0),
        );
    }

    fn unit_tile(&mut self, root: Entity, unit: &UnitView) {
        match unit.art {
            Some(path) => {
                let box_size = self.tile_box();
                self.art(root, path, unit.focus, false, box_size, NOMINAL_CARD);
            }
            None => {
                let accent = self.palette.bg_slot;
                self.child(
                    root,
                    (fill_node(), BackgroundColor(accent), Pickable::IGNORE),
                );
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

        // Nothing else. The mock-up's `.slot.unit` carries its illustration
        // and, when the unit is wounded, `.dmg` — the stylesheet says so in as
        // many words ("only minimal cue on board: a thin HP bar shows ONLY
        // when damaged") and the page's own caption repeats it ("Carte cliquée
        // = infos + actions · sinon pleine illustration"). Status badges, the
        // `+n` equipment pill and the "INCLINÉ" veil all belong to the click,
        // i.e. to the popover and the card detail, which already show them.

        // Damaged → the thin HP bar. Mock-up `.dmg i`: the foe's is `var(--gd)`
        // and yours `var(--atk)`, so a wounded unit of yours reads pink.
        if unit.damaged {
            let color = if unit.is_you {
                self.palette.atk
            } else {
                self.palette.gold
            };
            let inset = self.metrics.chrome(l::SLOT_HP_BAR_INSET);
            let bar_row = self.child(
                root,
                (
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(inset),
                        right: px(inset),
                        bottom: px(self.metrics.chrome(4.0)),
                        flex_direction: FlexDirection::Row,
                        ..default()
                    },
                    Pickable::IGNORE,
                ),
            );
            let h = self.metrics.chrome(l::SLOT_HP_BAR_H);
            self.gauge(bar_row, h, unit.hp_ratio, color);
        }
    }

    fn captain_token(&mut self, root: Entity, token: &CaptainTokenView) {
        if let Some(path) = token.art {
            let box_size = self.tile_box();
            self.art(root, path, token.focus, false, box_size, NOMINAL_CARD);
        }
        self.shade(root, 0.1, 0.9);

        let foe = self.palette.foe;
        let poppins = self.fonts.poppins_semi.clone();
        let badge = self.child(
            root,
            (
                Node {
                    position_type: PositionType::Absolute,
                    top: px(self.metrics.chrome(2.0)),
                    left: px(self.metrics.chrome(4.0)),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        self.child(badge, caption("VERSO", &poppins, l::FS_TINY, foe, 2.0));

        let bottom = self.child(
            root,
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: px(self.metrics.chrome(5.0)),
                    right: px(self.metrics.chrome(5.0)),
                    bottom: px(self.metrics.chrome(4.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(self.metrics.chrome(3.0)),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        let cinzel = self.fonts.cinzel_bold.clone();
        let white = self.palette.text;
        self.child(
            bottom,
            text(token.name.clone(), &cinzel, l::FS_LABEL, white),
        );
        // The verso token is a captain: its bar follows the captain rule.
        let color = self.palette.captain_hp_color(token.is_you, token.hp_ratio);
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
        let bar_h = self.metrics.chrome(l::SLOT_HP_BAR_H);
        self.gauge(row, bar_h, token.hp_ratio, color);
    }

    // --------------------------------------------------------
    // Command bar
    // --------------------------------------------------------

    pub fn captain_content(&mut self, root: Entity, captain: &CaptainCardView) {
        if let Some(path) = captain.art {
            let box_size = Vec2::new(self.metrics.captain_w, self.metrics.cmd_h);
            self.art(root, path, captain.focus, false, box_size, NOMINAL_CARD);
        }
        self.shade(root, 0.15, 0.92);

        // ATK / DEF chips, top-right. Like a tile's chrome, every fixed pixel
        // here goes through `metrics.chrome`: the card's box is scaled by
        // `HalfMetrics`, so leaving its insets, gauge and name in design pixels
        // is what makes a shrunk command bar drift from the mock-up's 116x96
        // `.capcard`.
        let chips = self.child(
            root,
            (
                Node {
                    position_type: PositionType::Absolute,
                    top: px(self.metrics.chrome(4.0)),
                    right: px(self.metrics.chrome(5.0)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::End,
                    row_gap: px(self.metrics.chrome(2.0)),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        // Mock-up `.capcard .ad` — compact glyph chips (`⚔9` / `🛡4`), not the
        // words "ATK"/"DEF", so the stack never widens over the portrait.
        let ink = self.palette.bg_deep.with_alpha(0.7);
        let atk = self.palette.atk;
        let def = self.palette.def;
        let chip_fs = self.metrics.chrome(l::FS_TINY);
        self.glyph_chip(chips, CROSSED_SWORDS, captain.atk, chip_fs, atk, ink);
        self.glyph_chip(chips, SHIELD, captain.def, chip_fs, def, ink);

        // Role / name / PV, bottom-left.
        let info = self.child(
            root,
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: px(self.metrics.chrome(7.0)),
                    right: px(self.metrics.chrome(7.0)),
                    bottom: px(self.metrics.chrome(5.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(self.metrics.chrome(2.0)),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        let poppins = self.fonts.poppins.clone();
        let cinzel = self.fonts.cinzel_bold.clone();
        let dim = self.palette.text_dim;
        let white = self.palette.text;
        let role = if captain.flipped {
            "CAPITAINE ★ VERSO"
        } else {
            "CAPITAINE"
        };
        self.child(
            info,
            caption(role, &poppins, self.metrics.chrome(l::FS_TINY), dim, 2.0),
        );
        let name_fs = self.metrics.chrome(l::FS_NAME);
        self.child(info, text(captain.name.clone(), &cinzel, name_fs, white));

        let hp_row = self.child(
            info,
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(self.metrics.chrome(5.0)),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        // Mock-up: the foe's captain bar is flat `--foe` whatever its fill, and
        // yours follows the green HP ramp — a full-health enemy captain must
        // never read green.
        let color = self
            .palette
            .captain_hp_color(captain.is_you, captain.hp_ratio);
        let gauge_h = self.metrics.chrome(7.0);
        self.gauge(hp_row, gauge_h, captain.hp_ratio, color);
        let poppins_bold = self.fonts.poppins_bold.clone();
        let pv_fs = self.metrics.chrome(l::FS_STAT);
        self.child(
            hp_row,
            text(captain.pv.to_string(), &poppins_bold, pv_fs, color),
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
                    let box_size = Vec2::new(self.metrics.ship_w, self.metrics.cmd_h);
                    self.art(root, path, Focus::CENTER, false, box_size, NOMINAL_CARD);
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
                let font = self.fonts.poppins.clone();
                self.child(label, text(name.clone(), &font, l::FS_TINY, cyan));
            }
            // Mock-up `.shipslot.empty` — dashed, `⚓` over `Navire`.
            (None, _) => {
                let faded = cyan.with_alpha(0.6);
                self.dashed_outline(root, faded.with_alpha(0.35), 1.0);
                let box_ = self.child(
                    root,
                    (
                        Node {
                            width: percent(100.0),
                            height: percent(100.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            row_gap: px(1.0),
                            ..default()
                        },
                        Pickable::IGNORE,
                    ),
                );
                let symbols = self.symbols.clone();
                self.child(box_, text(ANCHOR, &symbols, l::FS_LABEL, faded));
                let font = self.fonts.poppins_semi.clone();
                self.child(box_, caption("Navire", &font, l::FS_TINY, faded, 1.0));
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
        let poppins = self.fonts.poppins.clone();
        let cinzel = self.fonts.cinzel_bold.clone();
        let dim = self.palette.text_dim;
        let gold = self.palette.gold;
        self.child(row, caption("VOLONTÉ", &poppins, l::FS_TINY, dim, 1.8));

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

    /// Mock-up `.counts` — `<span>{HAND} 6</span><span>🂠 44</span>`, the glyphs
    /// being why `.counts span` stays narrow inside `.res`.
    ///
    /// These two pills are the **only** hand / deck counts the client draws:
    /// the footer's old "MAIN n ──── DECK n" head strip showed the same two
    /// numbers a second time, and the mock-up's footer has nothing of the sort.
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
        self.glyph_pill(row, HAND_GLYPH, res.hand, l::FS_LABEL, dim, wash);
        self.glyph_pill(row, CARD_BACK, res.deck, l::FS_LABEL, dim, wash);
    }

    // --------------------------------------------------------
    // Header
    // --------------------------------------------------------

    /// Mock-up `header`: `.brand` on the left, `.tn` (ring + "TOUR") centred by
    /// `margin: 0 auto`, `.you-turn` — a **solid** gradient pill with dark
    /// text — on the right. Nothing else: the foe's counts and ship live in the
    /// foe command bar, which already draws them.
    pub fn header_content(&mut self, root: Entity, header: &HeaderView) {
        let gold = self.palette.gold;
        let amber = self.palette.amber;
        let cinzel = self.fonts.cinzel_bold.clone();
        let dark = self.palette.text_on_gold;

        // Mock-up `.brand{background:linear-gradient(90deg,#ffd36b,#ff8a5b);
        // -webkit-background-clip:text;color:transparent}`. Bevy text cannot be
        // filled with a gradient (there is no clip-to-glyph pass), so the ramp
        // is walked letter by letter: five runs, gold on the left, amber on the
        // right — the same picture at this size, and it costs no render target.
        let brand = self.child(
            root,
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        let letters: Vec<char> = "TCGOP".chars().collect();
        let last = (letters.len().saturating_sub(1)).max(1) as f32;
        for (i, letter) in letters.iter().enumerate() {
            let t = i as f32 / last;
            let color = mix(gold, amber, t);
            self.child(brand, text(letter.to_string(), &cinzel, l::FS_TITLE, color));
        }

        // `margin: 0 auto` on the turn group = a flexible spacer either side.
        self.spacer(root);

        let turn = self.child(
            root,
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(8.0),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        );
        let ring = self.child(
            turn,
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
        // Mock-up `.tn{font-family:'Cinzel'}` covers the digit **and** the word.
        self.child(turn, caption("TOUR", &cinzel, l::FS_LABEL, gold, 0.9));

        self.spacer(root);

        // The "À toi" pill: a solid gradient with dark text, not a wash.
        let tone = tone_color(self.palette, header.status.tone);
        let tag = self.child(
            root,
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(4.0),
                    padding: UiRect::axes(px(11.0), px(5.0)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundGradient::from(LinearGradient::to_right(vec![
                    ColorStop::new(tone, percent(0.0)),
                    ColorStop::new(darken(tone, 0.78), percent(100.0)),
                ])),
                Pickable::IGNORE,
            ),
        );
        let font = self.fonts.poppins_bold.clone();
        if let Some(glyph) = header.status.glyph {
            let symbols = self.symbols.clone();
            self.child(tag, text(glyph, &symbols, l::FS_LABEL, dark));
        }
        self.child(tag, text(header.status.text, &font, l::FS_LABEL, dark));
    }

    /// A `flex-grow: 1` gap — the flexbox equivalent of `margin: 0 auto`.
    fn spacer(&mut self, parent: Entity) -> Entity {
        self.child(
            parent,
            (
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
                Pickable::IGNORE,
            ),
        )
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock-up: the front line is `V1..V3` and the back line `B1..B3`, even
    /// though the engine names the back slots `A1..A3`.
    #[test]
    fn the_back_line_placeholders_read_b() {
        assert_eq!(slot_code(Slot::V1), "V1");
        assert_eq!(slot_code(Slot::V2), "V2");
        assert_eq!(slot_code(Slot::V3), "V3");
        assert_eq!(slot_code(Slot::A1), "B1");
        assert_eq!(slot_code(Slot::A2), "B2");
        assert_eq!(slot_code(Slot::A3), "B3");
    }

    /// The dashes are percentages of their edge, so a tick and a gap always
    /// alternate — a fixed 6 px tick welded itself into a solid line as soon as
    /// the edge was shorter than `ticks * 6`, which the empty ship slot was.
    #[test]
    fn dashes_leave_a_gap_at_every_size() {
        for ticks in [3usize, 7] {
            let span = 100.0 / (2 * ticks - 1) as f32;
            let ink = span * ticks as f32;
            assert!(ink < 100.0, "{ticks} ticks cover the whole edge");
            // Ink and gaps are equal, which is what "dashed" looks like.
            assert!((ink - (100.0 - ink) - span).abs() < 1e-3);
        }
    }

    /// The glyphs the mock-up's `.counts` uses must exist in the one symbol
    /// face the client ships, or the pill would render a blank.
    #[test]
    fn the_count_glyphs_are_single_symbols() {
        for glyph in [HAND_GLYPH, CARD_BACK] {
            assert_eq!(glyph.chars().count(), 1, "{glyph:?}");
        }
        assert_eq!(CARD_BACK, "\u{1F0A0}", "the mock-up's card back");
    }
}

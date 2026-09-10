//! The shared furniture of every modal: the scrim, the gold-on-navy panel, the
//! section boxes, the pills and the buttons.
//!
//! Buttons never carry a closure. They carry a [`ClickCommand`] component and
//! one global observer ([`super::PanelsPlugin`]'s `route_panel_clicks`) turns it
//! into `resetUI()` + [`DispatchAction`](crate::bridge::DispatchAction), so the
//! whole click path is the same one the board and the hand use.

use bevy::prelude::*;
use bevy::ui::widget::Text;

use crate::app::{Fonts, Palette, layout as L};
use crate::selection::UiCommand;

// ============================================================
// Context
// ============================================================

/// Handles the panel builders need, bundled so they are not threaded one by one.
pub struct PanelCtx<'a> {
    pub palette: &'a Palette,
    pub fonts: &'a Fonts,
    /// DejaVu Sans — the only shipped face with ⚔ / ★ / ⚓ / 👁 coverage.
    pub symbols: &'a Handle<Font>,
}

// ============================================================
// Click routing
// ============================================================

/// What clicking this node does. Read by `route_panel_clicks`.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ClickCommand(pub UiCommand);

/// Marks the panel body: a click that reaches it stops there instead of
/// bubbling to the scrim (TS `onClick={(e) => e.stopPropagation()}`).
#[derive(Component, Debug, Clone, Copy)]
pub struct SwallowClicks;

/// A hoverable button: `base` is its resting fill, `hover` the lit one.
///
/// `gradient` carries the two stops when the skin is a ramp rather than a flat
/// fill (mock-up `.pop .acts button{background:linear-gradient(135deg,…)}`):
/// a `BackgroundGradient` paints over `BackgroundColor`, so the hover pass has
/// to rewrite the ramp instead of the colour or the button would not light up
/// at all.
#[derive(Component, Debug, Clone, Copy)]
pub struct PanelButton {
    pub base: Color,
    pub hover: Color,
    pub gradient: Option<(Color, Color)>,
}

impl PanelButton {
    /// The stops to paint for the current pointer state.
    pub fn stops(&self, hovered: bool) -> Option<(Color, Color)> {
        let (from, to) = self.gradient?;
        Some(if hovered {
            (lighten(from), lighten(to))
        } else {
            (from, to)
        })
    }
}

/// Mock-up `linear-gradient(135deg, …)` — the fill of every call to action.
pub fn button_gradient(from: Color, to: Color) -> BackgroundGradient {
    BackgroundGradient::from(LinearGradient::to_bottom_right(vec![
        ColorStop::new(from, percent(0.)),
        ColorStop::new(to, percent(100.)),
    ]))
}

// ============================================================
// Glyphs (DejaVu Sans)
// ============================================================

pub const SWORD: &str = "\u{2694}";
pub const STAR: &str = "\u{2605}";
pub const SPARKLE: &str = "\u{2726}";
pub const BOLT: &str = "\u{26A1}";
pub const ANCHOR: &str = "\u{2693}";
pub const EYE: &str = "\u{25C9}";
pub const SHIELD: &str = "\u{26E8}";
pub const CROWN: &str = "\u{265B}";
pub const CROSS: &str = "\u{2715}";
pub const HEART: &str = "\u{2665}";

// ============================================================
// Text
// ============================================================

/// A wrapping text node (the default layout) — descriptions and prose.
pub fn body(value: impl Into<String>, font: &Handle<Font>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font: font.clone().into(),
            font_size: FontSize::Px(size.max(5.0)),
            ..default()
        },
        TextColor(color),
        Pickable::IGNORE,
    )
}

/// A single-line text node — names, stats, captions.
pub fn line(value: impl Into<String>, font: &Handle<Font>, size: f32, color: Color) -> impl Bundle {
    (body(value, font, size, color), TextLayout::no_wrap())
}

// ============================================================
// Containers
// ============================================================

/// The full-screen dimmer. The caller decides whether clicking it closes the
/// modal by inserting a [`ClickCommand`] (TS's backdrop `onClick={onClose}`) or
/// a [`SwallowClicks`] (the counter window, which has no backdrop handler).
pub fn scrim(palette: &Palette, z: i32) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: px(0.),
            right: px(0.),
            top: px(0.),
            bottom: px(0.),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            padding: UiRect::all(px(16.)),
            ..default()
        },
        BackgroundColor(palette.scrim),
        GlobalZIndex(z),
    )
}

/// An **invisible** full-screen catcher: the popover has no dimmer (the
/// mock-up's `.pop` sits straight on the board), but a click outside it must
/// still close it, exactly like a modal backdrop.
pub fn catcher(z: i32) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: px(0.),
            right: px(0.),
            top: px(0.),
            bottom: px(0.),
            ..default()
        },
        BackgroundColor(Color::NONE),
        GlobalZIndex(z),
    )
}

/// Mock-up `.pop` — the info + actions card anchored on a selected tile.
pub fn popover(palette: &Palette, at: Vec2) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: px(at.x),
            top: px(at.y),
            width: px(L::POPOVER_W),
            flex_direction: FlexDirection::Column,
            row_gap: px(6.),
            padding: UiRect::axes(px(10.), px(9.)),
            border: UiRect::all(px(1.)),
            border_radius: BorderRadius::all(px(14.)),
            ..default()
        },
        BackgroundColor(palette.bg_panel),
        BorderColor::all(palette.gold.with_alpha(0.5)),
        SwallowClicks,
    )
}

/// Marks a panel body whose content may be taller than the window, so the
/// wheel can move it (see `panels::scroll_panels`).
#[derive(Component, Debug, Clone, Copy)]
pub struct ScrollArea;

/// The gold-edged navy panel itself.
///
/// The body **scrolls** rather than clipping: the window is resizable down to
/// `min_window_h()`, which leaves a modal about 690 px tall, and a card detail
/// stacks a full `CardFace` (~350 px) plus name, quote, trait pills, ability
/// sections and wrapped rules prose. Clipping that silently cuts the tail of a
/// long card off with no scrollbar, no fade and no way to reach it.
pub fn panel(palette: &Palette, width: f32, edge: Color) -> impl Bundle {
    (
        Node {
            width: px(width.min(L::PANEL_MAX_W)),
            // Relative to the full-screen scrim, so a modal follows the window
            // instead of being capped at the design height.
            max_height: percent(92.),
            flex_direction: FlexDirection::Column,
            row_gap: px(10.),
            padding: UiRect::all(px(L::PANEL_PAD)),
            border: UiRect::all(px(2.)),
            border_radius: BorderRadius::all(px(L::PANEL_RADIUS)),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        ScrollPosition::default(),
        ScrollArea,
        BackgroundColor(palette.bg_panel),
        BorderColor::all(edge),
        SwallowClicks,
    )
}

/// How many logical pixels one wheel "line" moves a panel.
pub const SCROLL_LINE: f32 = 22.0;

/// A tinted rounded box — the "Équipement" / "Effet" / "Synergies" cards.
pub fn section(tint: Color, edge: Option<Color>) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(2.),
            padding: UiRect::axes(px(9.), px(7.)),
            border: UiRect::all(px(if edge.is_some() { 1. } else { 0. })),
            border_radius: BorderRadius::all(px(10.)),
            ..default()
        },
        BackgroundColor(tint),
        BorderColor::all(edge.unwrap_or(Color::NONE)),
        Pickable::IGNORE,
    )
}

/// A row that lays its children out horizontally with a gap.
pub fn row(gap: f32) -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: px(gap),
        ..default()
    }
}

/// A column that lays its children out vertically with a gap.
pub fn column(gap: f32) -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(gap),
        ..default()
    }
}

// ============================================================
// Small parts
// ============================================================

/// A rounded pill — element chips, attack traits, status badges.
pub fn spawn_pill(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    label: impl Into<String>,
    fg: Color,
    bg: Color,
    size: f32,
) {
    parent.spawn((
        Node {
            padding: UiRect::axes(px(6.), px(1.)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(bg),
        Pickable::IGNORE,
        children![line(label, font, size, fg)],
    ));
}

/// The section caption: tiny, upper-case, letter-spaced Oswald.
pub fn spawn_caption(
    parent: &mut ChildSpawnerCommands,
    ctx: &PanelCtx,
    label: impl Into<String>,
    color: Color,
) {
    parent.spawn((
        line(label, &ctx.fonts.oswald_bold, L::FS_TINY + 1.0, color),
        bevy::text::LetterSpacing::Px(1.0),
    ));
}

/// A horizontal PV gauge, `ratio` in `0..=1`.
pub fn spawn_gauge(parent: &mut ChildSpawnerCommands, ratio: f32, fill: Color, height: f32) {
    parent.spawn((
        Node {
            width: percent(100.),
            height: px(height),
            border_radius: BorderRadius::MAX,
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(Color::srgba(1., 1., 1., 0.10)),
        Pickable::IGNORE,
        children![(
            Node {
                width: percent((ratio.clamp(0., 1.) * 100.).max(0.)),
                height: percent(100.),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(fill),
        )],
    ));
}

// ============================================================
// Buttons
// ============================================================

/// The six button skins of the web (`btn-gold`, `btn-danger`, `btn-ghost` and
/// the three inline gradients of the counter window / ship menu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonTone {
    Gold,
    Danger,
    Ghost,
    Blue,
    Purple,
    Cyan,
}

impl ButtonTone {
    /// The two stops of the mock-up's gradient skins, if this tone has one.
    ///
    /// `.btn` / `.pop .acts button` are `gold → amber`, `.sp` is
    /// `--atk → #ff5d7a`; every other tone is a flat panel fill.
    fn gradient(self, palette: &Palette) -> Option<(Color, Color)> {
        match self {
            ButtonTone::Gold => Some((palette.gold, palette.amber)),
            ButtonTone::Danger => Some((palette.atk, palette.atk_deep)),
            _ => None,
        }
    }

    /// `(fill, label, border)`.
    fn colors(self, palette: &Palette) -> (Color, Color, Color) {
        match self {
            ButtonTone::Gold => (palette.gold, palette.text_on_gold, palette.gold_deep),
            // Mock-up `.pop .acts .sp{background:linear-gradient(135deg,#ff7a8a,#ff5d7a);
            // color:#fff}` — the pink of `--atk`, not a brick red of its own.
            ButtonTone::Danger => (palette.atk, Color::WHITE, palette.atk_deep),
            ButtonTone::Ghost => (
                Color::srgba(1., 1., 1., 0.07),
                palette.text,
                Color::srgba(1., 1., 1., 0.18),
            ),
            ButtonTone::Blue => (
                Color::srgb(0.16, 0.38, 0.80),
                Color::WHITE,
                Color::srgb(0.08, 0.22, 0.54),
            ),
            ButtonTone::Purple => (
                Color::srgb(0.52, 0.28, 0.86),
                Color::WHITE,
                Color::srgb(0.30, 0.11, 0.58),
            ),
            ButtonTone::Cyan => (
                Color::srgb(0.12, 0.58, 0.70),
                Color::WHITE,
                Color::srgb(0.05, 0.29, 0.36),
            ),
        }
    }
}

/// One button of a panel.
#[derive(Debug, Clone)]
pub struct ButtonSpec {
    pub label: String,
    pub tone: ButtonTone,
    pub disabled: bool,
    pub command: UiCommand,
    pub height: f32,
    pub font_size: f32,
    /// Optional glyph drawn before the label, in the symbol face.
    pub glyph: Option<&'static str>,
    /// Stretch to the row's free space (`flex-1` in the TSX).
    pub grow: bool,
}

impl ButtonSpec {
    pub fn new(label: impl Into<String>, tone: ButtonTone, command: UiCommand) -> Self {
        ButtonSpec {
            label: label.into(),
            tone,
            disabled: false,
            command,
            height: 34.0,
            font_size: L::FS_BODY,
            glyph: None,
            grow: false,
        }
    }

    /// The gold call-to-action size of the mock-up.
    pub fn tall(mut self) -> Self {
        self.height = L::BUTTON_H;
        self
    }

    pub fn glyph(mut self, glyph: &'static str) -> Self {
        self.glyph = Some(glyph);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn grow(mut self) -> Self {
        self.grow = true;
        self
    }
}

/// Spawn `spec` as a child of `parent`.
///
/// A disabled button is drawn at half opacity and carries **no**
/// [`ClickCommand`], so it is inert exactly like the TSX `disabled` attribute.
pub fn spawn_button(parent: &mut ChildSpawnerCommands, ctx: &PanelCtx, spec: ButtonSpec) {
    let (fill, label_color, border) = spec.tone.colors(ctx.palette);
    let alpha = if spec.disabled { 0.45 } else { 1.0 };
    let fill = fill.with_alpha(fill.alpha() * alpha);
    let label_color = label_color.with_alpha(alpha);
    let gradient = spec
        .tone
        .gradient(ctx.palette)
        .map(|(from, to)| (from.with_alpha(alpha), to.with_alpha(alpha)));

    let mut button = parent.spawn((
        Node {
            height: px(spec.height),
            min_height: px(spec.height),
            flex_grow: if spec.grow { 1. } else { 0. },
            flex_shrink: 0.,
            padding: UiRect::axes(px(12.), px(0.)),
            column_gap: px(6.),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border: UiRect::all(px(2.)),
            border_radius: BorderRadius::all(px(L::BUTTON_RADIUS)),
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(border.with_alpha(alpha)),
        Button,
    ));
    if let Some((from, to)) = gradient {
        button.insert(button_gradient(from, to));
    }

    if !spec.disabled {
        button.insert((
            ClickCommand(spec.command.clone()),
            PanelButton {
                base: fill,
                hover: lighten(fill),
                gradient,
            },
        ));
    }

    let glyph = spec.glyph;
    let label = spec.label.clone();
    let symbols = ctx.symbols.clone();
    // Mock-up: every call to action is `font-family:'Poppins';font-weight:700`
    // — `.btn` in the footer, `.pop .acts button` in the popover and the panel
    // buttons alike. Oswald here made the two families disagree with the
    // footer's own CTA, which already uses Poppins.
    let poppins = ctx.fonts.poppins_bold.clone();
    let size = spec.font_size;
    button.with_children(|inner| {
        if let Some(glyph) = glyph {
            inner.spawn(line(glyph, &symbols, size + 1.0, label_color));
        }
        inner.spawn(line(label, &poppins, size, label_color));
    });
}

/// The small `✕` in the corner of the card detail.
pub fn spawn_close_cross(parent: &mut ChildSpawnerCommands, ctx: &PanelCtx) {
    parent.spawn((
        Node {
            width: px(22.),
            height: px(22.),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::MAX,
            flex_shrink: 0.,
            ..default()
        },
        BackgroundColor(Color::srgba(1., 1., 1., 0.06)),
        Button,
        ClickCommand(UiCommand::Reset),
        PanelButton {
            gradient: None,
            base: Color::srgba(1., 1., 1., 0.06),
            hover: Color::srgba(1., 1., 1., 0.18),
        },
        children![line(CROSS, ctx.symbols, L::FS_BODY, ctx.palette.text_faint)],
    ));
}

/// A slightly brighter version of a fill, used as the hover state.
pub fn lighten(color: Color) -> Color {
    let srgba = color.to_srgba();
    Color::srgba(
        (srgba.red + 0.10).min(1.0),
        (srgba.green + 0.10).min(1.0),
        (srgba.blue + 0.10).min(1.0),
        (srgba.alpha + 0.06).min(1.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tone_reads_against_its_own_label() {
        let palette = Palette::default();
        for tone in [
            ButtonTone::Gold,
            ButtonTone::Danger,
            ButtonTone::Ghost,
            ButtonTone::Blue,
            ButtonTone::Purple,
            ButtonTone::Cyan,
        ] {
            let (fill, label, _) = tone.colors(&palette);
            assert_ne!(fill, label, "{tone:?} would be invisible");
        }
    }

    #[test]
    fn hover_never_leaves_the_unit_range() {
        let lit = lighten(Color::srgb(0.98, 0.98, 0.98));
        let srgba = lit.to_srgba();
        assert!(srgba.red <= 1.0 && srgba.green <= 1.0 && srgba.blue <= 1.0);
        assert!(srgba.alpha <= 1.0);
    }

    #[test]
    fn a_disabled_spec_keeps_its_command_for_inspection() {
        let spec = ButtonSpec::new("x", ButtonTone::Gold, UiCommand::Reset).disabled(true);
        assert!(spec.disabled);
        assert_eq!(spec.command, UiCommand::Reset);
    }
}

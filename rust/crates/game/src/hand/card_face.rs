//! The card frame — a Bevy port of `src/components/FullCard.tsx`.
//!
//! One builder draws both sizes the hand needs:
//! - **compact** ([`CardScale::Compact`]) for the ~118 px cards of the fan:
//!   illustration, cost, PV, name, DEF and traits. The web literally
//!   `transform: scale()`s the whole 300x419 frame down to 118 px, which would
//!   put its rules text at ~4 px here — unreadable and expensive. The hover
//!   preview carries that text instead, exactly one pointer-move away.
//! - **full** ([`CardScale::Full`]) for the 250 px floating preview: the whole
//!   frame, passive / base action / special attack / effect line included.
//!
//! Everything is laid out from `scale = width / 300` so both sizes stay
//! proportional to the mock-up.

use bevy::prelude::*;
use bevy::ui::widget::Text;
use tcgop_engine::state::CardInstance;
use tcgop_engine::types::{
    AtkDefStat, AttackTrait, BaseAction, CardDef, CardType, CounterEffect, DamageTarget, Element,
    EventEffect, SpecialAttack, Trait,
};

use crate::app::{Fonts, Palette, layout as L};
use crate::art::{Focus, faction_visual, quote, rarity_border, trait_color, trait_label};
use crate::hand::layout::{CARD_FACE_FOCUS, NOMINAL_ART_ASPECT, cover_fit};

// ============================================================
// Inputs
// ============================================================

/// How much of the frame to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardScale {
    /// Hand-sized: art, cost, PV, name, DEF, traits.
    Compact,
    /// Preview-sized: everything, including the rules text.
    Full,
}

impl CardScale {
    /// The web renders the same frame at every width; the split is ours.
    pub fn for_width(width: f32) -> CardScale {
        if width >= 180.0 {
            CardScale::Full
        } else {
            CardScale::Compact
        }
    }

    pub fn is_full(self) -> bool {
        matches!(self, CardScale::Full)
    }
}

/// Everything the frame draws for one card.
#[derive(Debug, Clone)]
pub struct CardFace<'a> {
    pub def: &'a CardDef,
    /// The live instance, when the card is in play / in hand.
    pub instance: Option<&'a CardInstance>,
    /// Effective ATK (`getEffectiveAtk`, or `def.atk` off the board).
    pub atk: i32,
    /// Effective DEF (`getEffectiveDef`, or `def.def` off the board).
    pub def_value: i32,
    pub width: f32,
    pub scale: CardScale,
}

impl<'a> CardFace<'a> {
    /// A face with the definition's own stats — what a hand card shows.
    pub fn from_def(def: &'a CardDef, instance: Option<&'a CardInstance>, width: f32) -> Self {
        CardFace {
            def,
            instance,
            atk: def.atk.unwrap_or(0),
            def_value: def.def.unwrap_or(0),
            width,
            scale: CardScale::for_width(width),
        }
    }

    pub fn height(&self) -> f32 {
        self.width * L::CARD_ASPECT
    }

    fn s(&self) -> f32 {
        self.width / 300.0
    }

    fn is_character(&self) -> bool {
        self.def.card_type == CardType::Character
    }

    /// TS `instance ? instance.currentPv : def.pv`.
    fn current_pv(&self) -> i32 {
        self.instance
            .map(|i| i.current_pv)
            .unwrap_or_else(|| self.def.pv.unwrap_or(0))
    }

    /// TS `instance?.isAwakened` — which face of a Devil Fruit to show.
    pub fn is_flipped(&self) -> bool {
        self.instance
            .is_some_and(|i| i.is_awakened.unwrap_or(false))
    }
}

/// Handles + tokens the builder needs. Kept as one struct so callers do not
/// thread five system params through every helper.
pub struct FaceCtx<'a> {
    pub palette: &'a Palette,
    pub fonts: &'a Fonts,
    /// A font that actually has ⚔/★/♥ glyphs — none of Cinzel / Oswald /
    /// Spectral / Bangers ship anything above Latin-1.
    pub symbols: &'a Handle<Font>,
    /// Already-resolved illustration (`None` → the faction wash is drawn).
    pub art: Option<Handle<Image>>,
}

/// Marks the illustration node so [`super::sync_art_cover`] can re-crop it once
/// the JPEG's real dimensions are known.
#[derive(Component, Debug, Clone, Copy)]
pub struct ArtCover {
    pub box_w: f32,
    pub box_h: f32,
    pub focus: Focus,
    /// The `height / width` the current crop was computed for.
    pub applied_aspect: f32,
}

// ============================================================
// Builder
// ============================================================

const SHADOW: Color = Color::srgba(0.0, 0.0, 0.0, 0.9);

// Glyphs. None of Cinzel / Oswald / Spectral / Bangers ships anything past
// Latin-1, so every symbol below is rendered with `FaceCtx::symbols`
// (`assets/fonts/DejaVuSans.ttf`) and picked from what that font actually has —
// there is no shield, crown or hand glyph in it, hence the substitutes.
/// PV marker (U+2665).
pub const HEART: &str = "\u{2665}";
/// DEF marker — a filled pentagon standing in for the missing shield glyph.
pub const SHIELD: &str = "\u{2B1F}";
/// Base attack (U+2694).
pub const CROSSED_SWORDS: &str = "\u{2694}";
/// Support action (U+2726).
pub const SPARKLE: &str = "\u{2726}";
/// Special attack (U+2605).
pub const STAR: &str = "\u{2605}";
/// Ship (U+2693).
pub const ANCHOR: &str = "\u{2693}";
/// End of turn (U+27A1).
pub const ARROW_RIGHT: &str = "\u{27A1}";
/// Cancel (U+2715).
pub const CROSS: &str = "\u{2715}";
/// Log marker, your side (U+25BA).
pub const CARET_RIGHT: &str = "\u{25BA}";
/// Log marker, the opponent (U+25C4).
pub const CARET_LEFT: &str = "\u{25C4}";

fn font(handle: &Handle<Font>, px: f32) -> TextFont {
    TextFont {
        font: handle.clone().into(),
        font_size: FontSize::Px(px.max(5.0)),
        ..default()
    }
}

fn abs() -> Node {
    Node {
        position_type: PositionType::Absolute,
        ..default()
    }
}

/// Spawn the card frame as a child of `parent`, returning its root entity.
///
/// The frame is a single clipping node; the caller owns its outer transform
/// (the fan rotation) and any marker components it wants on the root.
pub fn spawn_card_face(parent: &mut ChildSpawnerCommands, ctx: &FaceCtx, face: &CardFace) -> Entity {
    let s = face.s();
    let w = face.width;
    let h = face.height();
    let pal = ctx.palette;
    let fac = faction_visual(face.def.faction);

    let mut root = parent.spawn((
        Node {
            width: px(w),
            height: px(h),
            overflow: Overflow::clip(),
            border: UiRect::all(px((2.0 * s).max(1.0))),
            border_radius: BorderRadius::all(px(16.0 * s)),
            ..default()
        },
        BackgroundColor(pal.bg_panel),
        BorderColor::all(rarity_border(face.def.rarity)),
    ));

    root.with_children(|card| {
        spawn_illustration(card, ctx, face, w, h, fac.ambiance);
        spawn_scrims(card, s);
        spawn_cost_and_pv(card, ctx, face, s);
        spawn_def_and_traits(card, ctx, face, s);
        spawn_text_block(card, ctx, face, s, fac.label);
    });

    root.id()
}

fn spawn_illustration(
    card: &mut ChildSpawnerCommands,
    ctx: &FaceCtx,
    face: &CardFace,
    w: f32,
    h: f32,
    wash: Color,
) {
    let _ = face;
    match ctx.art.clone() {
        Some(image) => {
            let fit = cover_fit(w, h, NOMINAL_ART_ASPECT, CARD_FACE_FOCUS);
            card.spawn((
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
                    focus: CARD_FACE_FOCUS,
                    applied_aspect: NOMINAL_ART_ASPECT,
                },
                Pickable::IGNORE,
            ));
        }
        None => {
            card.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.),
                    top: px(0.),
                    width: percent(100.),
                    height: percent(100.),
                    ..default()
                },
                BackgroundColor(wash),
                Pickable::IGNORE,
            ));
        }
    }
}

fn spawn_scrims(card: &mut ChildSpawnerCommands, s: f32) {
    // Top scrim, so the cost / PV numbers keep their contrast.
    card.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(0.),
            right: px(0.),
            top: px(0.),
            height: px(70.0 * s),
            ..default()
        },
        BackgroundGradient::from(LinearGradient::to_bottom(vec![
            ColorStop::new(Color::srgba(0.024, 0.035, 0.055, 0.60), percent(0.)),
            ColorStop::new(Color::NONE, percent(100.)),
        ])),
        Pickable::IGNORE,
    ));
    // Left scrim (the web hangs the name off it).
    card.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(0.),
            top: px(0.),
            bottom: px(0.),
            width: px(46.0 * s),
            ..default()
        },
        BackgroundGradient::from(LinearGradient::to_right(vec![
            ColorStop::new(Color::srgba(0.024, 0.035, 0.055, 0.55), percent(0.)),
            ColorStop::new(Color::NONE, percent(100.)),
        ])),
        Pickable::IGNORE,
    ));
    // The long fade the rules text sits on — no hard box, like the web.
    card.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(0.),
            right: px(0.),
            bottom: px(0.),
            height: percent(62.),
            ..default()
        },
        BackgroundGradient::from(LinearGradient::to_bottom(vec![
            ColorStop::new(Color::NONE, percent(0.)),
            ColorStop::new(Color::srgba(0.027, 0.035, 0.051, 0.40), percent(44.)),
            ColorStop::new(Color::srgba(0.027, 0.035, 0.051, 0.85), percent(76.)),
            ColorStop::new(Color::srgba(0.027, 0.035, 0.051, 0.95), percent(100.)),
        ])),
        Pickable::IGNORE,
    ));
}

fn spawn_cost_and_pv(card: &mut ChildSpawnerCommands, ctx: &FaceCtx, face: &CardFace, s: f32) {
    let pal = ctx.palette;
    let shadow = TextShadow {
        offset: Vec2::splat((1.5 * s).max(0.6)),
        color: SHADOW,
    };

    card.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(2.0 * s),
            left: px(11.0 * s),
            ..default()
        },
        Text::new(face.def.cost.to_string()),
        font(&ctx.fonts.oswald_bold, 48.0 * s),
        TextColor(pal.gold),
        shadow,
        Pickable::IGNORE,
    ));

    if !face.is_character() {
        return;
    }
    card.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(4.0 * s),
            right: px(10.0 * s),
            align_items: AlignItems::Start,
            column_gap: px(3.0 * s),
            ..default()
        },
        Pickable::IGNORE,
        children![
            (
                Text::new(face.current_pv().to_string()),
                font(&ctx.fonts.oswald_bold, 48.0 * s),
                TextColor(pal.gold),
                shadow,
            ),
            (
                Node {
                    margin: UiRect::top(px(9.0 * s)),
                    ..default()
                },
                Text::new(HEART),
                font(ctx.symbols, 18.0 * s),
                TextColor(Color::srgb(0.847, 0.271, 0.235)),
            ),
        ],
    ));
}

fn spawn_def_and_traits(card: &mut ChildSpawnerCommands, ctx: &FaceCtx, face: &CardFace, s: f32) {
    let traits: &[Trait] = face.def.traits.as_deref().unwrap_or(&[]);
    if !face.is_character() && traits.is_empty() {
        return;
    }
    let pal = ctx.palette;
    let def_value = face.def_value.to_string();
    let is_character = face.is_character();
    let traits: Vec<Trait> = traits.to_vec();
    let oswald_bold = ctx.fonts.oswald_bold.clone();
    let symbols = ctx.symbols.clone();

    card.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(9.0 * s),
            top: percent(54.),
            max_width: px(158.0 * s),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::End,
            row_gap: px(6.0 * s),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|col| {
        if is_character {
            col.spawn((
                Node {
                    align_items: AlignItems::Center,
                    column_gap: px(3.0 * s),
                    ..default()
                },
                children![
                    (
                        Text::new(SHIELD),
                        font(&symbols, 15.0 * s),
                        TextColor(pal.def),
                    ),
                    (
                        Text::new(def_value),
                        font(&oswald_bold, 22.0 * s),
                        TextColor(Color::WHITE),
                        TextShadow {
                            offset: Vec2::splat((1.2 * s).max(0.6)),
                            color: SHADOW,
                        },
                    ),
                ],
            ));
        }
        if traits.is_empty() {
            return;
        }
        col.spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            justify_content: JustifyContent::End,
            column_gap: px(4.0 * s),
            row_gap: px(3.0 * s),
            ..default()
        })
        .with_children(|row| {
            for t in traits {
                row.spawn((
                    Node {
                        padding: UiRect::axes(px(6.0 * s), px(1.0 * s)),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                    BackgroundColor(trait_color(t)),
                    children![(
                        Text::new(trait_label(t)),
                        font(&oswald_bold, 10.0 * s),
                        TextColor(Color::WHITE),
                    )],
                ));
            }
        });
    });
}

fn spawn_text_block(
    card: &mut ChildSpawnerCommands,
    ctx: &FaceCtx,
    face: &CardFace,
    s: f32,
    faction_label: &'static str,
) {
    let pal = ctx.palette;
    card.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(0.),
            right: px(0.),
            bottom: px(0.),
            padding: UiRect {
                left: px(38.0 * s),
                right: px(13.0 * s),
                top: px(0.),
                bottom: px(10.0 * s),
            },
            flex_direction: FlexDirection::Column,
            row_gap: px(1.0 * s),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|txt| {
        // Name — Cinzel, the house font for names and titles.
        txt.spawn((
            Text::new(face.def.name.clone()),
            font(&ctx.fonts.cinzel_bold, 21.0 * s),
            TextColor(Color::WHITE),
            TextShadow {
                offset: Vec2::splat((1.5 * s).max(0.6)),
                color: SHADOW,
            },
        ));

        if !face.scale.is_full() {
            return;
        }

        if let Some(q) = quote(&face.def.id) {
            txt.spawn((
                Text::new(format!("\u{201C}{q}\u{201D}")),
                font(&ctx.fonts.spectral, 11.0 * s),
                TextColor(pal.text_dim),
            ));
        }

        if let Some(passive) = &face.def.passive {
            txt.spawn((
                Text::new(passive.name.clone()),
                font(&ctx.fonts.spectral_bold, 12.5 * s),
                TextColor(Color::WHITE),
            ));
            txt.spawn((
                Text::new(passive.description.clone()),
                font(&ctx.fonts.spectral, 9.5 * s),
                TextColor(pal.text_dim),
            ));
        }

        if face.def.base_action.is_some() || face.def.special_attack.is_some() {
            txt.spawn((
                Node {
                    height: px(1.),
                    margin: UiRect::vertical(px(2.0 * s)),
                    ..default()
                },
                BackgroundGradient::from(LinearGradient::to_right(vec![
                    ColorStop::new(Color::srgba(1., 1., 1., 0.38), percent(0.)),
                    ColorStop::new(Color::NONE, percent(100.)),
                ])),
            ));
        }

        if let Some(base) = &face.def.base_action {
            spawn_action_row(txt, ctx, &ActionLine::base(base, face.atk), s);
        }
        if let Some(spec) = &face.def.special_attack {
            spawn_action_row(txt, ctx, &ActionLine::special(spec, face.atk), s);
        }

        if !face.is_character()
            && let Some(line) = effect_line(face.def)
        {
            txt.spawn((
                Text::new(line),
                font(&ctx.fonts.spectral, 10.0 * s),
                TextColor(pal.text_dim),
            ));
        }

        txt.spawn((
            Node {
                align_self: AlignSelf::End,
                margin: UiRect::top(px(1.0 * s)),
                ..default()
            },
            Text::new(format!("{} \u{00B7} {}", face.def.id, faction_label)),
            font(&ctx.fonts.oswald, 8.0 * s),
            TextColor(pal.text_faint),
        ));
    });
}

// ============================================================
// Action rows
// ============================================================

/// One row of the rules block — the union of TS `baseAction` / `specialAttack`
/// as `renderAction` sees it.
struct ActionLine<'a> {
    name: &'a str,
    description: Option<&'a str>,
    attack_traits: &'a [AttackTrait],
    element: Option<Element>,
    /// `None` for a support action (TS hides ATK when it heals).
    atk: Option<i32>,
    cost: i32,
    is_special: bool,
}

impl<'a> ActionLine<'a> {
    fn base(a: &'a BaseAction, atk: i32) -> ActionLine<'a> {
        let support = a.is_support.unwrap_or(false);
        let heals = support && a.heal_amount.is_some();
        ActionLine {
            name: &a.name,
            description: a.description.as_deref(),
            attack_traits: a.attack_traits.as_deref().unwrap_or(&[]),
            element: a.element,
            atk: (!heals).then_some(atk),
            cost: 0,
            is_special: false,
        }
    }

    fn special(a: &'a SpecialAttack, atk: i32) -> ActionLine<'a> {
        let heals = a.is_support.unwrap_or(false) && a.heal_amount.is_some();
        ActionLine {
            name: &a.name,
            description: a.description.as_deref(),
            attack_traits: a.attack_traits.as_deref().unwrap_or(&[]),
            element: a.element,
            atk: (!heals).then_some(atk + a.atk_bonus),
            cost: a.cost,
            is_special: true,
        }
    }
}

fn spawn_action_row(parent: &mut ChildSpawnerCommands, ctx: &FaceCtx, line: &ActionLine, s: f32) {
    let pal = ctx.palette;
    let accent = if line.is_special { pal.atk } else { pal.deploy };
    let icon = if line.is_special {
        STAR
    } else if line.atk.is_none() {
        SPARKLE
    } else {
        CROSSED_SWORDS
    };
    let name = line.name.to_string();
    let description = line.description.map(str::to_string);
    let cost = format!("{} Vol.", line.cost);
    let atk = line.atk.map(|v| format!("ATK {v}"));
    let chips: Vec<(String, Color)> = line
        .element
        .map(|e| (element_label(e).to_string(), element_color(e)))
        .into_iter()
        .chain(
            line.attack_traits
                .iter()
                .map(|t| (attack_trait_label(*t).to_string(), Color::srgba(1., 1., 1., 0.22))),
        )
        .collect();

    let oswald = ctx.fonts.oswald.clone();
    let oswald_bold = ctx.fonts.oswald_bold.clone();
    let spectral = ctx.fonts.spectral.clone();
    let spectral_bold = ctx.fonts.spectral_bold.clone();
    let symbols = ctx.symbols.clone();
    let dim = pal.text_dim;

    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Start,
            column_gap: px(6.0 * s),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                flex_direction: FlexDirection::Column,
                min_width: px(0.),
                flex_shrink: 1.,
                ..default()
            })
            .with_children(|left| {
                left.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    align_items: AlignItems::Center,
                    column_gap: px(5.0 * s),
                    row_gap: px(2.0 * s),
                    ..default()
                })
                .with_children(|head| {
                    head.spawn((
                        Text::new(icon),
                        font(&symbols, 11.0 * s),
                        TextColor(accent),
                    ));
                    head.spawn((
                        Text::new(name),
                        font(&spectral_bold, 12.5 * s),
                        TextColor(Color::WHITE),
                    ));
                    for (label, bg) in chips {
                        head.spawn((
                            Node {
                                padding: UiRect::axes(px(6.0 * s), px(0.)),
                                border_radius: BorderRadius::MAX,
                                ..default()
                            },
                            BackgroundColor(bg),
                            children![(
                                Text::new(label),
                                font(&oswald, 8.5 * s),
                                TextColor(Color::WHITE),
                            )],
                        ));
                    }
                });
                if let Some(desc) = description {
                    left.spawn((
                        Text::new(desc),
                        font(&spectral, 9.5 * s),
                        TextColor(dim),
                    ));
                }
            });

            row.spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::End,
                flex_shrink: 0.,
                ..default()
            })
            .with_children(|right| {
                right.spawn((
                    Text::new(cost),
                    font(&oswald, 10.0 * s),
                    TextColor(Color::srgba(1., 1., 1., 0.92)),
                ));
                if let Some(atk) = atk {
                    right.spawn((
                        Text::new(atk),
                        font(&oswald_bold, 10.0 * s),
                        TextColor(Color::srgba(1., 1., 1., 0.92)),
                    ));
                }
            });
        });
}

// ============================================================
// Labels
// ============================================================

/// TS `ELEMENT_LABEL`.
pub fn element_label(e: Element) -> &'static str {
    match e {
        Element::Fire => "Feu",
        Element::Water => "Eau",
        Element::Thunder => "Foudre",
        Element::Ice => "Glace",
        Element::Sand => "Sable",
        Element::Poison => "Poison",
    }
}

/// TS `ELEMENT_COLOR` (`src/lib/theme.ts`).
pub fn element_color(e: Element) -> Color {
    match e {
        Element::Fire => Color::srgb(0.910, 0.353, 0.212),
        Element::Water => Color::srgb(0.243, 0.573, 0.863),
        Element::Thunder => Color::srgb(0.949, 0.769, 0.259),
        Element::Ice => Color::srgb(0.529, 0.804, 0.906),
        Element::Sand => Color::srgb(0.788, 0.667, 0.404),
        Element::Poison => Color::srgb(0.588, 0.392, 0.804),
    }
}

/// TS `ATK_TRAIT` in `FullCard.tsx`.
pub fn attack_trait_label(t: AttackTrait) -> &'static str {
    match t {
        AttackTrait::Range => "Portée",
        AttackTrait::Piercing => "Perçant",
        AttackTrait::Zone => "Zone",
        AttackTrait::Total => "Total",
        AttackTrait::Impact => "Impact",
    }
}

/// The one-line rules text of a non-character card — TS `FullCard`'s
/// `!isChar` branch plus `describeEvent`.
pub fn effect_line(def: &CardDef) -> Option<String> {
    match def.card_type {
        CardType::Object => {
            let mut out = String::new();
            if let Some(atk) = def.bonus_atk.filter(|v| *v != 0) {
                out.push_str(&format!("+{atk} ATK "));
            }
            if let Some(d) = def.bonus_def.filter(|v| *v != 0) {
                out.push_str(&format!("+{d} DEF "));
            }
            if let Some(effect) = &def.equip_effect {
                out.push_str(effect);
            }
            let out = out.trim().to_string();
            (!out.is_empty()).then_some(out)
        }
        CardType::Ship => {
            let mut out = def.ship_passive.clone().unwrap_or_default();
            if let Some(active) = &def.ship_active {
                out.push_str(&format!(
                    " — {} ({}V)",
                    active.name, active.cost
                ));
            }
            let out = out.trim().to_string();
            (!out.is_empty()).then_some(out)
        }
        CardType::Counter => def.counter_effect.as_ref().map(describe_counter),
        CardType::Event => def.event_effect.as_ref().map(describe_event),
        CardType::Character => None,
    }
}

/// TS `describeEvent(e)` (the same five cases, then the effect's own text).
pub fn describe_event(effect: &EventEffect) -> String {
    match effect {
        EventEffect::GainWill { amount } => format!("Gagne +{amount} Volonté."),
        EventEffect::Draw { amount, discard } => match discard {
            Some(d) => format!("Pioche {amount}, défausse {d}."),
            None => format!("Pioche {amount}."),
        },
        EventEffect::HealAlly { amount, all_allies } => {
            if all_allies.unwrap_or(false) {
                format!("Tous les alliés +{amount} PV.")
            } else {
                format!("1 allié +{amount} PV.")
            }
        }
        EventEffect::BuffAllies { stat, amount, .. } => {
            format!("Alliés +{amount} {}.", stat_label(*stat))
        }
        EventEffect::DamageEnemies { amount, target, .. } => {
            format!("{amount} dégâts ({}).", damage_target_label(*target))
        }
        EventEffect::DodgeAll => "Esquive totale ce tour.".to_string(),
        EventEffect::Rally { atk, def, heal } => {
            format!("Ralliement : +{atk} ATK, +{def} DEF, +{heal} PV.")
        }
        EventEffect::BuffSingle { stat, amount, .. } => {
            format!("1 allié +{amount} {}.", stat_label(*stat))
        }
        EventEffect::RushBuff { atk } => format!("Rush : +{atk} ATK."),
        EventEffect::Tutor { .. } => "Cherche une carte dans ton deck.".to_string(),
        EventEffect::DeployTokens { count, .. } => format!("Déploie {count} jeton(s)."),
        EventEffect::HealAllBuff { heal, atk } => {
            format!("Tous les alliés +{heal} PV et +{atk} ATK.")
        }
        EventEffect::DebuffAllEnemies { atk, .. } => {
            format!("Tous les ennemis {atk} ATK.")
        }
        EventEffect::GrantHakiAll { atk } => match atk {
            Some(atk) => format!("Haki pour tous, +{atk} ATK."),
            None => "Haki pour tous.".to_string(),
        },
        EventEffect::Custom { description, .. } => description.clone(),
    }
}

/// TS `def.counterEffect.type === "reduceDamage" ? … : e.description`.
pub fn describe_counter(effect: &CounterEffect) -> String {
    match effect {
        CounterEffect::ReduceDamage { amount, .. } => format!("Réduit {amount} dégâts."),
        CounterEffect::Survive { description }
        | CounterEffect::Cancel { description, .. }
        | CounterEffect::Untargetable { description } => description.clone(),
    }
}

fn stat_label(stat: AtkDefStat) -> &'static str {
    match stat {
        AtkDefStat::Atk => "ATK",
        AtkDefStat::Def => "DEF",
    }
}

fn damage_target_label(target: DamageTarget) -> &'static str {
    match target {
        DamageTarget::AllFront => "ligne avant",
        DamageTarget::AllCursed => "maudits",
        DamageTarget::All => "tous",
        DamageTarget::Single => "une cible",
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tcgop_engine::types::{Faction, Rarity};

    #[test]
    fn the_scale_split_follows_the_two_widths_the_hand_uses() {
        assert_eq!(CardScale::for_width(L::HAND_CARD_W), CardScale::Compact);
        assert_eq!(CardScale::for_width(L::FULL_CARD_W), CardScale::Full);
    }

    #[test]
    fn event_lines_match_the_typescript_wording() {
        assert_eq!(
            describe_event(&EventEffect::GainWill { amount: 2 }),
            "Gagne +2 Volonté."
        );
        assert_eq!(
            describe_event(&EventEffect::Draw {
                amount: 2,
                discard: Some(1)
            }),
            "Pioche 2, défausse 1."
        );
        assert_eq!(
            describe_event(&EventEffect::Draw {
                amount: 2,
                discard: None
            }),
            "Pioche 2."
        );
        assert_eq!(
            describe_event(&EventEffect::HealAlly {
                amount: 3,
                all_allies: Some(true)
            }),
            "Tous les alliés +3 PV."
        );
        assert_eq!(
            describe_event(&EventEffect::DodgeAll),
            "Esquive totale ce tour."
        );
        assert_eq!(
            describe_event(&EventEffect::DamageEnemies {
                amount: 2,
                target: DamageTarget::AllFront,
                cursed_bonus: None,
                sand: None,
                destroy_ships: None,
            }),
            "2 dégâts (ligne avant)."
        );
    }

    #[test]
    fn counter_lines_special_case_damage_reduction() {
        assert_eq!(
            describe_counter(&CounterEffect::ReduceDamage {
                amount: 2,
                captain_bonus: None
            }),
            "Réduit 2 dégâts."
        );
        assert_eq!(
            describe_counter(&CounterEffect::Survive {
                description: "Survit à 1 PV.".into()
            }),
            "Survit à 1 PV."
        );
    }

    #[test]
    fn an_objects_effect_line_concatenates_its_bonuses() {
        let mut def = CardDef::new("X-1", "Sabre", CardType::Object, 1, Faction::Pirate, Rarity::C, "ST01");
        def.bonus_atk = Some(2);
        def.bonus_def = Some(1);
        def.equip_effect = Some("Ignore Bouclier.".into());
        assert_eq!(
            effect_line(&def).as_deref(),
            Some("+2 ATK +1 DEF Ignore Bouclier.")
        );
    }

    #[test]
    fn a_character_has_no_effect_line() {
        let def = CardDef::new("X-2", "Zoro", CardType::Character, 3, Faction::Pirate, Rarity::R, "ST01");
        assert_eq!(effect_line(&def), None);
    }

    #[test]
    fn a_support_action_hides_its_atk_like_the_web() {
        let mut base = BaseAction {
            name: "Soin".into(),
            atk: 0,
            ..Default::default()
        };
        base.is_support = Some(true);
        base.heal_amount = Some(2);
        assert!(ActionLine::base(&base, 4).atk.is_none());

        base.heal_amount = None;
        assert_eq!(ActionLine::base(&base, 4).atk, Some(4));
    }

    #[test]
    fn a_special_attack_adds_its_bonus_to_the_effective_atk() {
        let spec = SpecialAttack {
            name: "Onigiri".into(),
            cost: 3,
            atk_bonus: 3,
            ..Default::default()
        };
        let line = ActionLine::special(&spec, 5);
        assert_eq!(line.atk, Some(8));
        assert_eq!(line.cost, 3);
        assert!(line.is_special);
    }
}

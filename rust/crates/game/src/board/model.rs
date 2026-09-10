//! The board's **view model**: one pure snapshot of everything the terrain
//! draws, derived from [`Session`] + [`UiMode`].
//!
//! Rendering systems never read the engine directly: they build a [`BoardView`]
//! and diff it against what is already on screen, so a tile is only rebuilt
//! when something it shows actually changed. Because the whole thing is a plain
//! value, the board's behaviour is asserted head-lessly (see the tests at the
//! bottom): highlighting, dimming, HP bars, captain verso tokens…
//!
//! Reference: `renderHalf` / `renderLine` / `renderCommand` and the header of
//! `src/components/Game.tsx`.

use std::collections::BTreeSet;

use bevy::prelude::Color;
use tcgop_engine::types::{GameAction, PlayerId, Slot, StatusEffectType};

use crate::app::Palette;
use crate::art::{self, Focus};
use crate::board::geometry::{captain_focus, tile_focus};
use crate::bridge::Session;
use crate::selection::{
    self, CellHighlight, StatusHint, UiMode, attack_is_zone, attack_targets, captain_key,
    deploy_slots, equip_targets, status_hint, support_targets,
};

// ============================================================
// Rings
// ============================================================

/// The ring drawn around a cell / the captain card.
///
/// Priority follows `BoardSlot.tsx`: `impact ?? target ?? deploy`, with the
/// gold "this is what you are acting with" ring last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ring {
    #[default]
    None,
    /// Green — a character (or the captain verso) may land here; also the
    /// equipment holder ring, exactly like the TS (`isValidDeploy || isEquipTarget`).
    Deploy,
    /// Red, pulsing — a legal attack / support target.
    Target,
    /// Orange, pulsing — inside a zone attack's blast.
    Impact,
    /// Gold — the unit whose action is being aimed.
    Select,
}

impl Ring {
    /// `None` when nothing should be drawn.
    pub fn color(self, palette: &Palette) -> Option<Color> {
        match self {
            Ring::None => None,
            Ring::Deploy => Some(palette.deploy),
            Ring::Target => Some(palette.target),
            Ring::Impact => Some(palette.impact),
            Ring::Select => Some(palette.select),
        }
    }

    /// Attack rings breathe; placement rings are steady.
    pub fn pulses(self) -> bool {
        matches!(self, Ring::Target | Ring::Impact)
    }
}

/// `BoardSlot.tsx`'s ring choice, plus the gold "selected actor" ring.
pub fn ring_of(highlight: &CellHighlight, selected: bool) -> Ring {
    if highlight.impact {
        Ring::Impact
    } else if highlight.valid_target {
        Ring::Target
    } else if highlight.valid_deploy || highlight.equip_target {
        Ring::Deploy
    } else if selected {
        Ring::Select
    } else {
        Ring::None
    }
}

/// The id the current mode is acting *with* (attacker, support caster, the unit
/// whose action menu is open) — it gets the gold ring.
pub fn acting_id(mode: &UiMode) -> Option<&str> {
    match mode {
        UiMode::SelectingTarget { attacker_id, .. } => Some(attacker_id),
        UiMode::SelectingSupportTarget { instance_id } => Some(instance_id),
        UiMode::ActionMenu { instance_id } => Some(instance_id),
        _ => None,
    }
}

// ============================================================
// Cells
// ============================================================

/// A status effect as a coloured badge: the colour carries the meaning, the
/// number the remaining turns (`-1` = permanent, drawn as a bare dot).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatusBadge {
    pub effect: StatusEffectType,
    pub color: Color,
    pub turns: i32,
}

/// A character standing in a slot — a full illustration and nothing else.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitView {
    pub instance_id: String,
    pub def_id: String,
    /// Whose half the tile is on — the mock-up paints a wounded unit's `.dmg`
    /// bar `var(--atk)` on your side and `var(--gd)` on the foe's.
    pub is_you: bool,
    pub name: String,
    pub art: Option<&'static str>,
    pub focus: Focus,
    pub pv: i32,
    pub max_pv: i32,
    /// `current_pv < max_pv` — the only case where the thin HP bar shows.
    pub damaged: bool,
    pub hp_ratio: f32,
    pub tapped: bool,
    pub equipment: usize,
    pub badges: Vec<StatusBadge>,
    /// Rarity border (`RARITY_BORDER`).
    pub border: Color,
}

/// The captain's verso occupying one of its owner's slots.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptainTokenView {
    pub player: PlayerId,
    pub is_you: bool,
    pub def_id: String,
    pub name: String,
    pub art: Option<&'static str>,
    pub focus: Focus,
    pub pv: i32,
    pub max_pv: i32,
    pub hp_ratio: f32,
}

/// What sits in a cell.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum CellContent {
    #[default]
    Empty,
    Unit(UnitView),
    Captain(CaptainTokenView),
}

impl CellContent {
    /// The instance id (or captain key) a click on this cell would act on.
    // Used by the cell tests; the click path matches on `CellContent`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn occupant(&self) -> Option<&str> {
        match self {
            CellContent::Empty => None,
            CellContent::Unit(u) => Some(&u.instance_id),
            CellContent::Captain(_) => None,
        }
    }
}

/// How a cell is lit right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellDecor {
    pub ring: Ring,
    pub dimmed: bool,
}

/// One of the six cells of a side.
#[derive(Debug, Clone, PartialEq)]
pub struct CellView {
    pub slot: Slot,
    pub is_you: bool,
    pub content: CellContent,
    pub decor: CellDecor,
}

// ============================================================
// Command bar
// ============================================================

/// The captain card of a command bar (always shown, recto **or** verso stats).
#[derive(Debug, Clone, PartialEq)]
pub struct CaptainCardView {
    pub player: PlayerId,
    pub is_you: bool,
    pub def_id: String,
    pub name: String,
    pub art: Option<&'static str>,
    pub focus: Focus,
    pub atk: i32,
    pub def: i32,
    pub pv: i32,
    pub max_pv: i32,
    pub hp_ratio: f32,
    pub tapped: bool,
    pub flipped: bool,
    pub accent: Color,
    pub decor: CellDecor,
}

/// The ship slot of a command bar.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShipView {
    pub instance_id: Option<String>,
    pub name: Option<String>,
    pub art: Option<&'static str>,
}

/// Volonté + hand / deck counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceView {
    pub is_you: bool,
    pub volonte: i32,
    /// Lit pips, capped at [`WILL_PIPS`](crate::app::layout::WILL_PIPS).
    pub pips: usize,
    pub hand: usize,
    pub deck: usize,
}

/// Captain + ship + resources.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandView {
    pub captain: CaptainCardView,
    pub ship: ShipView,
    pub resources: ResourceView,
}

// ============================================================
// Halves and header
// ============================================================

/// One player's half of the flat terrain.
#[derive(Debug, Clone, PartialEq)]
pub struct HalfView {
    pub player: PlayerId,
    pub is_you: bool,
    /// Ship-deck floor art.
    pub floor: &'static str,
    pub command: CommandView,
    /// `V1..V3`, the line at the waterline.
    pub front: Vec<CellView>,
    /// `A1..A3`, the line farthest from the waterline.
    pub back: Vec<CellView>,
}

impl HalfView {
    pub fn cell(&self, slot: Slot) -> Option<&CellView> {
        self.front
            .iter()
            .chain(self.back.iter())
            .find(|c| c.slot == slot)
    }
}

/// The top bar.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderView {
    pub turn: u32,
    pub status: StatusHint,
    /// TS: the CTA is disabled while the AI plays or a counter window is open.
    pub can_end_turn: bool,
    /// "Annuler" only shows when the UI is not idle.
    pub can_cancel: bool,
}

/// Everything the board draws, in one comparable value.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardView {
    pub header: HeaderView,
    pub foe: HalfView,
    pub you: HalfView,
}

impl BoardView {
    pub fn half(&self, is_you: bool) -> &HalfView {
        if is_you { &self.you } else { &self.foe }
    }
}

// ============================================================
// Builder
// ============================================================

/// Build the whole snapshot. Pure: `(session, mode) -> BoardView`.
pub fn board_view(session: &Session, mode: &UiMode) -> BoardView {
    let ai = session.ai_player();
    let human = session.human;
    let valid: &[GameAction] = &session.valid;

    let deploy = deploy_slots(mode, valid);
    let attack = attack_targets(mode, valid, ai);
    let support = support_targets(mode, valid);
    let equip = equip_targets(mode, valid);
    let zone = attack_is_zone(mode, &session.state, &session.registry);
    let acting = acting_id(mode);

    let ctx = Ctx {
        session,
        mode,
        deploy,
        attack,
        support,
        equip,
        zone,
        acting,
    };

    BoardView {
        header: header_view(session, mode),
        foe: half_view(&ctx, ai, false),
        you: half_view(&ctx, human, true),
    }
}

/// Everything the per-cell computation needs, gathered once.
struct Ctx<'a> {
    session: &'a Session,
    mode: &'a UiMode,
    deploy: BTreeSet<Slot>,
    attack: BTreeSet<String>,
    support: BTreeSet<String>,
    equip: BTreeSet<String>,
    zone: bool,
    acting: Option<&'a str>,
}

impl Ctx<'_> {
    fn decor(&self, slot: Slot, is_you: bool, occupant: Option<&str>) -> CellDecor {
        let highlight = selection::cell_highlight(
            self.mode,
            slot,
            is_you,
            occupant,
            &self.deploy,
            &self.attack,
            &self.support,
            &self.equip,
            self.zone,
        );
        let selected = matches!((occupant, self.acting), (Some(id), Some(act)) if id == act);
        CellDecor {
            ring: ring_of(&highlight, selected),
            // TS dims every non-eligible cell, the attacker's own included; we
            // keep the unit you are acting *with* bright so its gold ring reads.
            dimmed: highlight.dimmed && !selected,
        }
    }
}

/// Mock-up `header`: the turn, the status pill, and the two flags the footer's
/// CTA row reads. The foe's crest, hand / deck counts and ship are **not** here
/// — the foe command bar already draws all three, and the mock-up shows them
/// exactly once.
fn header_view(session: &Session, mode: &UiMode) -> HeaderView {
    HeaderView {
        turn: session.state.turn_number,
        status: status_hint(mode, session.is_ai_turn(), session.in_counter_window()),
        can_end_turn: !session.is_ai_turn() && !session.in_counter_window(),
        can_cancel: !mode.is_idle(),
    }
}

fn half_view(ctx: &Ctx, player: PlayerId, is_you: bool) -> HalfView {
    let session = ctx.session;
    let state = &session.state;
    let ps = state.player(player);
    let visual = session
        .registry
        .captain_def(&ps.captain.def_id)
        .map(|def| art::faction_visual(def.faction))
        .unwrap_or_else(|| art::faction_visual(tcgop_engine::types::Faction::Pirate));

    let line = |slots: &[Slot]| -> Vec<CellView> {
        slots
            .iter()
            .map(|slot| cell_view(ctx, player, is_you, *slot))
            .collect()
    };

    HalfView {
        player,
        is_you,
        floor: visual.ship_deck,
        command: command_view(ctx, player, is_you),
        front: line(&Slot::FRONT),
        back: line(&Slot::BACK),
    }
}

fn cell_view(ctx: &Ctx, player: PlayerId, is_you: bool, slot: Slot) -> CellView {
    let session = ctx.session;
    let state = &session.state;
    let ps = state.player(player);

    // The captain's verso occupies one of its owner's slots (renderLine's first
    // branch): it is drawn as a compact art token and answers to captain clicks.
    if ps.captain.flipped && ps.captain.slot == Some(slot) {
        let key = captain_key(player);
        let decor = ctx.decor(slot, is_you, Some(&key));
        let content = session
            .registry
            .captain_def(&ps.captain.def_id)
            .map(|def| {
                let max_pv = def.verso.pv.max(1);
                CellContent::Captain(CaptainTokenView {
                    player,
                    is_you,
                    def_id: def.id.clone(),
                    name: def.name.clone(),
                    art: art::art_for(&def.id, true),
                    focus: tile_focus(&def.id),
                    pv: ps.captain.current_pv,
                    max_pv,
                    hp_ratio: ratio(ps.captain.current_pv, max_pv),
                })
            })
            .unwrap_or_default();
        return CellView {
            slot,
            is_you,
            content,
            decor,
        };
    }

    let occupant = ps.board.get(slot).cloned();
    let decor = ctx.decor(slot, is_you, occupant.as_deref());
    let content = occupant
        .as_deref()
        .and_then(|id| state.card(id))
        .and_then(|instance| {
            let def = session.registry.card_def(&instance.def_id)?;
            let max_pv = def.pv.unwrap_or(1).max(1);
            Some(CellContent::Unit(UnitView {
                instance_id: instance.instance_id.clone(),
                def_id: instance.def_id.clone(),
                is_you,
                name: def.name.clone(),
                art: art::art_for(&instance.def_id, instance.is_awakened.unwrap_or(false)),
                focus: tile_focus(&instance.def_id),
                pv: instance.current_pv,
                max_pv,
                damaged: instance.current_pv < max_pv,
                hp_ratio: ratio(instance.current_pv, max_pv),
                tapped: instance.tapped,
                equipment: instance.attached_objects.len(),
                badges: instance
                    .status_effects
                    .iter()
                    .map(|e| StatusBadge {
                        effect: e.effect_type,
                        color: crate::board::style::status_color(e.effect_type),
                        turns: e.turns_remaining,
                    })
                    .collect(),
                border: art::rarity_border(def.rarity),
            }))
        })
        .unwrap_or_default();

    CellView {
        slot,
        is_you,
        content,
        decor,
    }
}

fn command_view(ctx: &Ctx, player: PlayerId, is_you: bool) -> CommandView {
    let session = ctx.session;
    let state = &session.state;
    let ps = state.player(player);
    let captain = &ps.captain;

    let key = captain_key(player);
    // The command card is never a deploy slot; only the attack branch can light
    // it up, so `cell_highlight` is fed the captain's own slot for the dim rule.
    let highlight = selection::cell_highlight(
        ctx.mode,
        captain.slot.unwrap_or(Slot::V1),
        // Decision §8.28 (follow-up): the captain can be an *equip* target now,
        // and `cell_highlight` only lights one on the player's own side. The
        // attack and impact branches are the other side's and are unaffected —
        // your own captain key is never in `attack_targets`.
        is_you,
        Some(&key),
        &BTreeSet::new(),
        &ctx.attack,
        &ctx.support,
        &ctx.equip,
        ctx.zone,
    );
    let is_target = !is_you && highlight.valid_target;
    let selected = ctx.acting == Some(key.as_str());
    let decor = CellDecor {
        ring: if is_target {
            Ring::Target
        } else if highlight.equip_target {
            Ring::Deploy
        } else if selected {
            Ring::Select
        } else {
            Ring::None
        },
        dimmed: ctx.mode.is_selecting() && !is_target && !highlight.equip_target,
    };

    let (name, def_id, atk, def, max_pv, accent) =
        match session.registry.captain_def(&captain.def_id) {
            Some(cap) => {
                let (side_atk, side_def, side_pv) = if captain.flipped {
                    (cap.verso.atk, cap.verso.def, cap.verso.pv)
                } else {
                    (cap.recto.atk, cap.recto.def, cap.recto.pv)
                };
                (
                    cap.name.clone(),
                    cap.id.clone(),
                    side_atk,
                    side_def,
                    side_pv.max(1),
                    art::faction_visual(cap.faction).accent,
                )
            }
            None => (
                captain.def_id.clone(),
                captain.def_id.clone(),
                0,
                0,
                1,
                art::faction_visual(tcgop_engine::types::Faction::Pirate).accent,
            ),
        };

    let ship = ps
        .active_ship
        .as_ref()
        .and_then(|id| {
            let instance = state.card(id)?;
            let def = session.registry.card_def(&instance.def_id)?;
            Some(ShipView {
                instance_id: Some(id.clone()),
                name: Some(def.name.clone()),
                art: art::card_art(&def.id),
            })
        })
        .unwrap_or_default();

    CommandView {
        captain: CaptainCardView {
            player,
            is_you,
            art: art::art_for(&def_id, captain.flipped),
            focus: captain_focus(&def_id),
            def_id,
            name,
            atk,
            def,
            pv: captain.current_pv,
            max_pv,
            hp_ratio: ratio(captain.current_pv, max_pv),
            tapped: captain.tapped,
            flipped: captain.flipped,
            accent,
            decor,
        },
        ship,
        resources: ResourceView {
            is_you,
            volonte: ps.volonte,
            pips: (ps.volonte.max(0) as usize).min(crate::app::layout::WILL_PIPS),
            hand: ps.hand.len(),
            deck: ps.deck.len(),
        },
    }
}

fn ratio(current: i32, max: i32) -> f32 {
    if max <= 0 {
        return 0.0;
    }
    (current as f32 / max as f32).clamp(0.0, 1.0)
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::AutoAction;
    use crate::bridge::testkit::{advance, session as make_session};
    use crate::selection::StatusTone;
    use tcgop_engine::types::Zone;

    fn session() -> Session {
        make_session(7)
    }

    /// Play the human side for real — deploy whatever is affordable, end the
    /// turn otherwise, let the AI answer — until a `BaseAttack` is legal.
    fn drive_to_attack(session: &mut Session) -> Option<(String, String, bool)> {
        for _ in 0..600 {
            if session.winner().is_some() {
                return None;
            }
            let found = session.valid.iter().find_map(|a| match a {
                GameAction::BaseAttack {
                    attacker_instance_id,
                    target_instance_id,
                    target_is_captain,
                } => Some((
                    attacker_instance_id.clone(),
                    target_instance_id.clone(),
                    target_is_captain.unwrap_or(false),
                )),
                _ => None,
            });
            if found.is_some() {
                return found;
            }
            match session.needs_auto_action() {
                AutoAction::None if session.in_counter_window() => {
                    session.dispatch(GameAction::PassCounter).ok()?;
                }
                AutoAction::None => {
                    let next = session
                        .valid
                        .iter()
                        .find(|a| matches!(a, GameAction::DeployCharacter { .. }))
                        .cloned()
                        .unwrap_or(GameAction::EndTurn);
                    session.dispatch(next).ok()?;
                }
                _ => {
                    advance(session, 1, |_| false);
                }
            }
        }
        None
    }

    /// Drive the game until the human can deploy, then return the first
    /// `DeployCharacter` action of `valid`.
    fn until_deploy(session: &mut Session) -> (String, Slot) {
        assert!(
            advance(session, 200, |s| s
                .valid
                .iter()
                .any(|a| matches!(a, GameAction::DeployCharacter { .. }))),
            "the human must eventually afford a character"
        );
        session
            .valid
            .iter()
            .find_map(|a| match a {
                GameAction::DeployCharacter { instance_id, slot } => {
                    Some((instance_id.clone(), *slot))
                }
                _ => None,
            })
            .expect("checked above")
    }

    #[test]
    fn a_fresh_board_is_empty_on_both_sides() {
        let s = session();
        let view = board_view(&s, &UiMode::Idle);

        for half in [&view.you, &view.foe] {
            assert_eq!(half.front.len(), 3);
            assert_eq!(half.back.len(), 3);
            for cell in half.front.iter().chain(half.back.iter()) {
                assert_eq!(cell.content, CellContent::Empty);
                assert_eq!(cell.decor, CellDecor::default());
            }
        }
        assert_eq!(view.you.front[0].slot, Slot::V1);
        assert_eq!(view.you.back[2].slot, Slot::A3);
        assert!(view.you.is_you && !view.foe.is_you);
    }

    #[test]
    fn each_half_uses_its_own_faction_floor() {
        let s = session();
        let view = board_view(&s, &UiMode::Idle);
        assert_eq!(view.you.floor, art::MUGIWARA_DECK_IMAGE);
        assert_eq!(view.foe.floor, art::MARINE_DECK_IMAGE);
    }

    #[test]
    fn the_command_bar_shows_the_recto_of_each_captain() {
        let s = session();
        let view = board_view(&s, &UiMode::Idle);

        let you = &view.you.command.captain;
        let cap = s.registry.captain_def(&s.you().captain.def_id).unwrap();
        assert_eq!(you.name, cap.name);
        assert_eq!((you.atk, you.def), (cap.recto.atk, cap.recto.def));
        assert_eq!(you.max_pv, cap.recto.pv);
        assert_eq!(you.hp_ratio, 1.0);
        assert!(!you.flipped);
        assert_eq!(you.art, art::card_art(&cap.id));

        assert_eq!(view.you.command.resources.hand, s.you().hand.len());
        assert_eq!(view.you.command.resources.deck, s.you().deck.len());
        assert!(view.you.command.ship.instance_id.is_none());
    }

    #[test]
    fn the_header_reports_the_turn_and_the_status_only() {
        let mut s = session();
        let view = board_view(&s, &UiMode::Idle);
        assert_eq!(view.header.turn, s.state.turn_number);
        assert_eq!(view.header.status.tone, StatusTone::Ready);
        assert!(view.header.can_end_turn);
        assert!(!view.header.can_cancel);
        // The foe's counts belong to the foe command bar, and are drawn there
        // exactly once.
        assert_eq!(view.foe.command.resources.hand, s.foe().hand.len());
        assert_eq!(view.foe.command.resources.deck, s.foe().deck.len());

        // Handing the turn over disables the button and flips the tag.
        s.dispatch(GameAction::EndTurn).unwrap();
        let view = board_view(&s, &UiMode::Idle);
        assert!(!view.header.can_end_turn);
        assert_eq!(view.header.status.tone, StatusTone::Waiting);
        assert!(view.header.status.pulse);
    }

    #[test]
    fn cancelling_is_offered_as_soon_as_the_ui_leaves_idle() {
        let s = session();
        let mode = UiMode::SelectingSlot {
            card_id: "whatever".into(),
        };
        assert!(board_view(&s, &mode).header.can_cancel);
    }

    #[test]
    fn deploy_mode_lights_the_legal_slots_and_dims_everything_else() {
        let mut s = session();
        let (card_id, slot) = until_deploy(&mut s);
        let mode = UiMode::SelectingSlot { card_id };
        let view = board_view(&s, &mode);

        let cell = view.you.cell(slot).expect("slot exists");
        assert_eq!(cell.decor.ring, Ring::Deploy);
        assert!(!cell.decor.dimmed);

        // The enemy half can never be a deploy target, so it is dimmed.
        for cell in view.foe.front.iter().chain(view.foe.back.iter()) {
            assert_eq!(cell.decor.ring, Ring::None);
            assert!(cell.decor.dimmed);
        }
        assert!(view.foe.command.captain.decor.dimmed);
    }

    #[test]
    fn a_deployed_unit_shows_its_illustration_and_no_hp_bar_until_hurt() {
        let mut s = session();
        let (card_id, slot) = until_deploy(&mut s);
        s.dispatch(GameAction::DeployCharacter {
            instance_id: card_id.clone(),
            slot,
        })
        .unwrap();

        let view = board_view(&s, &UiMode::Idle);
        let CellContent::Unit(unit) = &view.you.cell(slot).unwrap().content else {
            panic!("the slot must hold the deployed character");
        };
        assert_eq!(unit.instance_id, card_id);
        assert!(!unit.damaged, "a fresh unit shows no HP bar");
        assert_eq!(unit.hp_ratio, 1.0);
        assert_eq!(unit.equipment, 0);
        assert!(unit.badges.is_empty());
        assert_eq!(unit.art, art::card_art(&unit.def_id));
        assert_eq!(unit.focus, tile_focus(&unit.def_id));

        // Wound it: the bar appears and the ratio follows.
        let max = unit.max_pv;
        let id = unit.instance_id.clone();
        s.state.cards.get_mut(&id).unwrap().current_pv = max - 1;
        let view = board_view(&s, &UiMode::Idle);
        let CellContent::Unit(unit) = &view.you.cell(slot).unwrap().content else {
            unreachable!()
        };
        assert!(unit.damaged);
        assert!(unit.is_you, "the tile knows whose half it is on");
        assert!(unit.hp_ratio < 1.0);
    }

    #[test]
    fn statuses_equipment_and_tapping_reach_the_tile() {
        let mut s = session();
        let (card_id, slot) = until_deploy(&mut s);
        s.dispatch(GameAction::DeployCharacter {
            instance_id: card_id.clone(),
            slot,
        })
        .unwrap();

        {
            let card = s.state.cards.get_mut(&card_id).unwrap();
            card.tapped = true;
            card.attached_objects.push("obj-1".into());
            card.status_effects.push(tcgop_engine::types::StatusEffect {
                effect_type: StatusEffectType::Burn,
                turns_remaining: 2,
                damage_per_turn: 1,
                source: "test".into(),
            });
        }

        let view = board_view(&s, &UiMode::Idle);
        let CellContent::Unit(unit) = &view.you.cell(slot).unwrap().content else {
            unreachable!()
        };
        assert!(unit.tapped);
        assert_eq!(unit.equipment, 1);
        assert_eq!(unit.badges.len(), 1);
        assert_eq!(unit.badges[0].turns, 2);
        assert_eq!(
            unit.badges[0].color,
            crate::board::style::status_color(StatusEffectType::Burn)
        );
    }

    #[test]
    fn aiming_an_attack_rings_the_target_and_selects_the_attacker() {
        let mut s = session();
        let (attacker, target, on_captain) =
            drive_to_attack(&mut s).expect("the human must eventually be able to attack");

        let mode = UiMode::SelectingTarget {
            attacker_id: attacker.clone(),
            is_special: false,
        };
        let view = board_view(&s, &mode);

        // The attacker wears the gold ring on your side.
        let mine = view
            .you
            .front
            .iter()
            .chain(view.you.back.iter())
            .find(|c| c.content.occupant() == Some(attacker.as_str()))
            .expect("the attacker is on the board");
        assert_eq!(mine.decor.ring, Ring::Select);
        assert!(!mine.decor.dimmed);

        if on_captain {
            assert_eq!(view.foe.command.captain.decor.ring, Ring::Target);
        } else {
            let hit = view
                .foe
                .front
                .iter()
                .chain(view.foe.back.iter())
                .find(|c| c.content.occupant() == Some(target.as_str()))
                .expect("the target is on the enemy board");
            assert!(matches!(hit.decor.ring, Ring::Target | Ring::Impact));
            assert!(!hit.decor.dimmed);
        }
    }

    #[test]
    fn a_flipped_captain_becomes_a_token_in_its_slot() {
        let mut s = session();
        {
            let captain = &mut s.state.players.player1.captain;
            captain.flipped = true;
            captain.slot = Some(Slot::V2);
        }
        let view = board_view(&s, &UiMode::Idle);
        let CellContent::Captain(token) = &view.you.cell(Slot::V2).unwrap().content else {
            panic!("V2 must hold the captain verso");
        };
        assert_eq!(token.player, PlayerId::Player1);
        let cap = s.registry.captain_def(&s.you().captain.def_id).unwrap();
        assert_eq!(token.name, cap.name);
        assert_eq!(token.max_pv, cap.verso.pv);
        assert_eq!(token.art, art::art_for(&cap.id, true));
        // …and the command bar now reads the verso stats.
        assert!(view.you.command.captain.flipped);
        assert_eq!(view.you.command.captain.atk, cap.verso.atk);
    }

    #[test]
    fn an_active_ship_fills_the_ship_slot() {
        let mut s = session();
        let ship_id = s
            .state
            .cards
            .values()
            .find(|c| {
                c.owner == PlayerId::Player1
                    && s.registry
                        .card_def(&c.def_id)
                        .is_some_and(|d| d.card_type == tcgop_engine::types::CardType::Ship)
            })
            .map(|c| c.instance_id.clone())
            .expect("the mugiwara deck ships a ship");
        s.state.cards.get_mut(&ship_id).unwrap().zone = Zone::Board;
        s.state.players.player1.active_ship = Some(ship_id.clone());

        let view = board_view(&s, &UiMode::Idle);
        assert_eq!(view.you.command.ship.instance_id, Some(ship_id));
        assert!(view.you.command.ship.name.is_some());
        assert!(view.you.command.ship.art.is_some());
    }

    #[test]
    fn volonte_pips_saturate_at_ten() {
        let mut s = session();
        s.state.players.player1.volonte = 42;
        let view = board_view(&s, &UiMode::Idle);
        assert_eq!(view.you.command.resources.volonte, 42);
        assert_eq!(
            view.you.command.resources.pips,
            crate::app::layout::WILL_PIPS
        );
    }

    #[test]
    fn ring_priority_matches_the_web_client() {
        let full = CellHighlight {
            valid_deploy: true,
            valid_target: true,
            equip_target: true,
            impact: true,
            dimmed: false,
        };
        assert_eq!(ring_of(&full, true), Ring::Impact);
        assert_eq!(
            ring_of(
                &CellHighlight {
                    impact: false,
                    ..full
                },
                true
            ),
            Ring::Target
        );
        assert_eq!(
            ring_of(
                &CellHighlight {
                    impact: false,
                    valid_target: false,
                    ..full
                },
                true
            ),
            Ring::Deploy
        );
        assert_eq!(ring_of(&CellHighlight::default(), true), Ring::Select);
        assert_eq!(ring_of(&CellHighlight::default(), false), Ring::None);
        assert!(Ring::Target.pulses() && Ring::Impact.pulses());
        assert!(!Ring::Deploy.pulses() && !Ring::Select.pulses());
        assert!(Ring::None.color(&Palette::default()).is_none());
    }
}

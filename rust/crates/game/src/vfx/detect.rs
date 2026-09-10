//! Combat detection — a port of `src/lib/useCombatVfx.ts`.
//!
//! The web hook diffs two snapshots of the engine state and turns the deltas
//! into visual events; nothing here reads the *actions*, so an attack that the
//! engine resolves through three different code paths still produces exactly
//! one impact per PV loss.
//!
//! This module is pure: [`snapshot`] and [`diff`] never touch the ECS, so the
//! whole detection layer is unit-tested head-lessly. Screen positions are
//! resolved later, by the render systems, from
//! [`BoardAnchors`](crate::board::BoardAnchors) — the events only carry engine
//! ids (`instance_id`, or `captain_playerN`).

use std::collections::{BTreeMap, BTreeSet};

use tcgop_engine::registry::CardRegistry;
use tcgop_engine::state::{GameState, PendingAttack};
use tcgop_engine::types::{AttackTrait, PlayerId, Zone};

use crate::selection::captain_key;
use crate::vfx::VfxElement;

// ============================================================
// Events
// ============================================================

/// TS `VfxKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VfxKind {
    /// A projectile flying from the attacker to its target.
    Attack,
    /// A burst + a damage number on a unit that just lost PV.
    Impact,
    /// A sparkle + a healing number.
    Heal,
    /// A unit left the board.
    Ko,
    /// The attack-name banner of a special / captain attack.
    Banner,
    /// A unit was deployed — the landing slam.
    Spawn,
}

/// How hard the board shakes on a hit (TS `vfx-shake` / `vfx-shake-hard`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shake {
    /// 360 ms, small amplitude.
    Soft,
    /// 520 ms, big amplitude — captains, specials, and any hit of 7+.
    Hard,
}

/// The token flash the web toggles on the unit's DOM node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Flash {
    /// `vfx-lunge` — the attacker leans toward its target.
    Lunge,
    /// `vfx-hit` — a red flash on a damaged unit.
    Hit,
    /// `vfx-knockback` — `impact` attacks push the tile back.
    Knockback,
    /// `vfx-heal` — a green pulse.
    Heal,
    /// `vfx-slam` — the landing punch of a deploy.
    Slam,
}

impl Flash {
    /// Duration of the flash, in milliseconds (`CombatVfxLayer` timings).
    pub const fn millis(self) -> u64 {
        match self {
            Flash::Lunge => 380,
            Flash::Hit | Flash::Knockback => 520,
            Flash::Heal => 700,
            Flash::Slam => 430,
        }
    }
}

/// One visual event — TS `VfxEvent`, with ids instead of screen coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct CombatEvent {
    pub kind: VfxKind,
    pub element: VfxElement,
    /// Engine id the effect starts from (the attacker).
    pub from_id: Option<String>,
    /// Engine id the effect lands on.
    pub to_id: Option<String>,
    /// Damage (positive) or healing (positive) amount.
    pub value: Option<i32>,
    pub is_special: bool,
    /// The attack splashes (`zone` / `total`).
    pub zone: bool,
    /// The attack knocks back (`impact` / `pushback`).
    pub impact: bool,
    /// Captain or special — the grander presentation.
    pub big: bool,
    /// Banner text (the attack name).
    pub label: Option<String>,
    /// Banner subtitle (the attacker name).
    pub sub: Option<String>,
    /// Cut-in: the attacker's def id, for its illustration.
    pub def_id: Option<String>,
    pub attacker_is_captain: bool,
    pub attacker_name: Option<String>,
    /// Cut-in: the attack name (special attacks only).
    pub attack_name: Option<String>,
    /// Board shake this event asks for.
    pub shake: Option<Shake>,
    /// Tile flash this event asks for, on [`CombatEvent::to_id`] (or, for
    /// [`VfxKind::Attack`], on [`CombatEvent::from_id`]).
    pub flash: Option<Flash>,
}

impl CombatEvent {
    fn new(kind: VfxKind, element: VfxElement) -> Self {
        CombatEvent {
            kind,
            element,
            from_id: None,
            to_id: None,
            value: None,
            is_special: false,
            zone: false,
            impact: false,
            big: false,
            label: None,
            sub: None,
            def_id: None,
            attacker_is_captain: false,
            attacker_name: None,
            attack_name: None,
            shake: None,
            flash: None,
        }
    }

    /// The id the flash applies to.
    pub fn flash_target(&self) -> Option<&str> {
        match self.kind {
            VfxKind::Attack => self.from_id.as_deref(),
            _ => self.to_id.as_deref(),
        }
    }

    /// A cut-in is played for a big attack whose attacker is identified — TS
    /// `CutInLayer` (`ev.kind === "attack" && ev.big && ev.attackerName`).
    pub fn wants_cut_in(&self) -> bool {
        self.kind == VfxKind::Attack && self.big && self.attacker_name.is_some()
    }
}

// ============================================================
// Snapshot
// ============================================================

/// The three things the diff compares — TS `Snapshot`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// `instance_id` → current PV, for the units **on the board**.
    pub pv: BTreeMap<String, i32>,
    /// `player1` / `player2` captain PV.
    pub captain_pv: BTreeMap<PlayerId, i32>,
    /// Which instances are on the board.
    pub board: BTreeSet<String>,
    /// TS `pendingSig` — `attacker>target:damage:special`, or `None`.
    pub pending_sig: Option<String>,
}

/// TS `snapshot(state)`.
pub fn snapshot(state: &GameState) -> Snapshot {
    let mut pv = BTreeMap::new();
    let mut board = BTreeSet::new();
    for (id, card) in &state.cards {
        if card.zone == Zone::Board {
            pv.insert(id.clone(), card.current_pv);
            board.insert(id.clone());
        }
    }
    let mut captain_pv = BTreeMap::new();
    for player in [PlayerId::Player1, PlayerId::Player2] {
        captain_pv.insert(player, state.player(player).captain.current_pv);
    }
    Snapshot {
        pv,
        captain_pv,
        board,
        pending_sig: state.pending_attack.as_ref().map(pending_sig),
    }
}

/// TS `` `${pa.attackerId}>${pa.targetId}:${pa.rawDamage}:${pa.isSpecial}` ``.
pub fn pending_sig(pending: &PendingAttack) -> String {
    format!(
        "{}>{}:{}:{}",
        pending.attacker_id, pending.target_id, pending.raw_damage, pending.is_special
    )
}

/// TS `targetId(pa, state)` — the on-screen id a pending attack lands on.
pub fn attack_target_id(pending: &PendingAttack, state: &GameState) -> String {
    if pending.target_is_captain {
        let owner = match crate::selection::captain_key_owner(&pending.attacker_id) {
            Some(player) => Some(player),
            None => state
                .cards
                .get(&pending.attacker_id)
                .map(|card| card.owner),
        };
        // No attacker in the state any more: fall back to the raw target id.
        return match owner {
            Some(owner) => captain_key(owner.opponent()),
            None => pending.target_id.clone(),
        };
    }
    pending.target_id.clone()
}

/// TS `attackLabel(state, pa)` — `(attack name, attacker name)`, special
/// attacks only.
pub fn attack_label(
    state: &GameState,
    registry: &CardRegistry,
    pending: &PendingAttack,
) -> (Option<String>, Option<String>) {
    if !pending.is_special {
        return (None, None);
    }
    if let Some(player) = crate::selection::captain_key_owner(&pending.attacker_id) {
        let Some(captain) = registry.captain_def(&state.player(player).captain.def_id) else {
            return (None, None);
        };
        return (
            Some(captain.verso.special_attack.name.clone()),
            Some(captain.name.clone()),
        );
    }
    let Some(def) = state
        .cards
        .get(&pending.attacker_id)
        .and_then(|card| registry.card_def(&card.def_id))
    else {
        return (None, None);
    };
    let name = def
        .special_attack
        .as_ref()
        .map(|special| special.name.clone())
        .unwrap_or_else(|| def.name.clone());
    (Some(name), Some(def.name.clone()))
}

/// `(def id, display name)` of an attacker, for the cut-in.
fn attacker_identity(
    state: &GameState,
    registry: &CardRegistry,
    attacker_id: &str,
) -> (Option<String>, Option<String>) {
    if let Some(player) = crate::selection::captain_key_owner(attacker_id) {
        return match registry.captain_def(&state.player(player).captain.def_id) {
            Some(captain) => (Some(captain.id.clone()), Some(captain.name.clone())),
            None => (None, None),
        };
    }
    match state
        .cards
        .get(attacker_id)
        .and_then(|card| registry.card_def(&card.def_id))
    {
        Some(def) => (Some(def.id.clone()), Some(def.name.clone())),
        None => (None, None),
    }
}

/// Element / impact / big of the attack a target was hit by — TS
/// `elementForTarget`.
fn look_for_target(
    state: &GameState,
    last_pending: Option<&PendingAttack>,
    id: &str,
) -> (VfxElement, bool, bool) {
    if let Some(pending) = last_pending {
        let target = attack_target_id(pending, state);
        if target == id || !pending.target_is_captain {
            let element =
                crate::vfx::element::element_for(pending.element, pending.has_haki);
            let impact = pending.attack_traits.contains(&AttackTrait::Impact)
                || pending.pushback.unwrap_or(false);
            let big = pending.is_special
                || crate::selection::is_captain_key(&pending.attacker_id);
            return (element, impact, big);
        }
    }
    (VfxElement::Physical, false, false)
}

// ============================================================
// Diff
// ============================================================

/// TS `useCombatVfx`'s effect body: everything that changed between `prev` and
/// the current `state`, as visual events, in the web's order (attack + banner,
/// unit deltas, captain deltas, KOs, spawns).
///
/// `last_pending` is the TS `lastPending` ref: the most recent pending attack
/// seen, kept even after the engine cleared it, so a hit that resolves one
/// frame later still gets its element.
pub fn diff(
    prev: &Snapshot,
    next: &Snapshot,
    state: &GameState,
    registry: &CardRegistry,
    last_pending: Option<&PendingAttack>,
) -> Vec<CombatEvent> {
    let mut out = Vec::new();

    // --- 1. an attack was just declared → projectile (+ banner) ---
    if let Some(pending) = state.pending_attack.as_ref()
        && next.pending_sig.is_some() && next.pending_sig != prev.pending_sig {
            let element = crate::vfx::element::element_for(pending.element, pending.has_haki);
            let is_captain = crate::selection::is_captain_key(&pending.attacker_id);
            let big = pending.is_special || is_captain;
            let (label, sub) = attack_label(state, registry, pending);
            let (def_id, attacker_name) =
                attacker_identity(state, registry, &pending.attacker_id);

            let mut attack = CombatEvent::new(VfxKind::Attack, element);
            attack.from_id = Some(pending.attacker_id.clone());
            attack.to_id = Some(attack_target_id(pending, state));
            attack.is_special = pending.is_special;
            attack.zone = pending.attack_traits.contains(&AttackTrait::Zone)
                || pending.attack_traits.contains(&AttackTrait::Total);
            attack.impact = pending.attack_traits.contains(&AttackTrait::Impact)
                || pending.pushback.unwrap_or(false);
            attack.big = big;
            attack.def_id = def_id;
            attack.attacker_is_captain = is_captain;
            attack.attacker_name = attacker_name;
            attack.attack_name = if pending.is_special { label.clone() } else { None };
            attack.flash = Some(Flash::Lunge);
            out.push(attack);

            if let Some(label) = label {
                let mut banner = CombatEvent::new(VfxKind::Banner, element);
                banner.label = Some(label);
                banner.sub = sub;
                banner.big = big;
                out.push(banner);
            }
        }

    // --- 2. PV deltas on the board ---
    let ids: BTreeSet<&String> = prev.pv.keys().chain(next.pv.keys()).collect();
    for id in ids {
        let (Some(before), Some(after)) = (prev.pv.get(id), next.pv.get(id)) else {
            continue;
        };
        let delta = after - before;
        if delta < 0 {
            let (element, impact, big) = look_for_target(state, last_pending, id);
            let damage = -delta;
            let mut event = CombatEvent::new(VfxKind::Impact, element);
            event.to_id = Some(id.clone());
            event.value = Some(damage);
            event.impact = impact;
            event.big = big;
            event.flash = Some(if impact { Flash::Knockback } else { Flash::Hit });
            event.shake = Some(if big || damage >= 7 {
                Shake::Hard
            } else {
                Shake::Soft
            });
            out.push(event);
        } else if delta > 0 {
            let mut event = CombatEvent::new(VfxKind::Heal, VfxElement::Physical);
            event.to_id = Some(id.clone());
            event.value = Some(delta);
            event.flash = Some(Flash::Heal);
            out.push(event);
        }
    }

    // --- 3. captain PV deltas (always a hard shake) ---
    for player in [PlayerId::Player1, PlayerId::Player2] {
        let before = prev.captain_pv.get(&player).copied().unwrap_or_default();
        let after = next.captain_pv.get(&player).copied().unwrap_or_default();
        let delta = after - before;
        let id = captain_key(player);
        if delta < 0 {
            let (element, impact, _) = look_for_target(state, last_pending, &id);
            let mut event = CombatEvent::new(VfxKind::Impact, element);
            event.to_id = Some(id);
            event.value = Some(-delta);
            event.impact = impact;
            event.big = true;
            event.flash = Some(Flash::Hit);
            event.shake = Some(Shake::Hard);
            out.push(event);
        } else if delta > 0 {
            let mut event = CombatEvent::new(VfxKind::Heal, VfxElement::Physical);
            event.to_id = Some(id);
            event.value = Some(delta);
            event.flash = Some(Flash::Heal);
            out.push(event);
        }
    }

    // --- 4. KO: a unit left the board ---
    for id in prev.board.difference(&next.board) {
        let mut event = CombatEvent::new(VfxKind::Ko, VfxElement::Physical);
        event.to_id = Some(id.clone());
        out.push(event);
    }

    // --- 5. spawn: a unit landed on the board ---
    for id in next.board.difference(&prev.board) {
        let mut event = CombatEvent::new(VfxKind::Spawn, VfxElement::Physical);
        event.to_id = Some(id.clone());
        event.flash = Some(Flash::Slam);
        out.push(event);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::testkit::{advance, session};
    use tcgop_engine::types::Element;

    fn pending(attacker: &str, target: &str, target_is_captain: bool) -> PendingAttack {
        PendingAttack {
            attacker_id: attacker.into(),
            target_id: target.into(),
            target_is_captain,
            is_special: false,
            raw_damage: 3,
            attack_power: None,
            element: None,
            attack_traits: Vec::new(),
            has_haki: false,
            ignore_shield: None,
            cannot_be_dodged: None,
            immobilize: None,
            sleep: None,
            pushback: None,
            pushback_slots: None,
            strip_stealth: None,
            survive_played: None,
            survive_target_id: None,
            damage_reduction: None,
            ignore_def: None,
            permanent_pv_loss: None,
            no_heal: None,
        }
    }

    #[test]
    fn a_fresh_snapshot_of_a_new_game_has_two_captains_and_no_board() {
        let session = session(7);
        let snap = snapshot(&session.state);
        assert_eq!(snap.captain_pv.len(), 2);
        assert!(snap.pending_sig.is_none());
        assert_eq!(snap.board.len(), snap.pv.len());
    }

    #[test]
    fn no_change_produces_no_event() {
        let session = session(7);
        let snap = snapshot(&session.state);
        let events = diff(&snap, &snap, &session.state, &session.registry, None);
        assert!(events.is_empty(), "an idle frame must be silent");
    }

    #[test]
    fn a_deployment_produces_a_slam() {
        let mut session = session(7);
        let before = snapshot(&session.state);
        assert!(
            advance(&mut session, 80, |s| s
                .state
                .cards
                .values()
                .any(|card| card.zone == Zone::Board)),
            "someone must deploy within 80 actions"
        );
        let after = snapshot(&session.state);
        let events = diff(&before, &after, &session.state, &session.registry, None);
        let spawn = events
            .iter()
            .find(|e| e.kind == VfxKind::Spawn)
            .expect("a unit reached the board");
        assert_eq!(spawn.flash, Some(Flash::Slam));
        assert!(spawn.to_id.is_some());
    }

    #[test]
    fn losing_pv_yields_a_damage_number_a_flash_and_a_shake() {
        let session = session(7);
        let mut before = snapshot(&session.state);
        before.pv.insert("unit-1".into(), 10);
        before.board.insert("unit-1".into());
        let mut after = before.clone();
        after.pv.insert("unit-1".into(), 3);

        let events = diff(&before, &after, &session.state, &session.registry, None);
        assert_eq!(events.len(), 1);
        let hit = &events[0];
        assert_eq!(hit.kind, VfxKind::Impact);
        assert_eq!(hit.value, Some(7));
        assert_eq!(hit.flash, Some(Flash::Hit));
        assert_eq!(hit.shake, Some(Shake::Hard), "7+ damage shakes hard");
        assert_eq!(hit.element, VfxElement::Physical);
    }

    #[test]
    fn a_small_hit_only_shakes_softly_and_a_heal_never_shakes() {
        let session = session(7);
        let mut before = Snapshot::default();
        before.pv.insert("u".into(), 10);
        before.board.insert("u".into());
        let mut hit = before.clone();
        hit.pv.insert("u".into(), 8);
        let events = diff(&before, &hit, &session.state, &session.registry, None);
        assert_eq!(events[0].shake, Some(Shake::Soft));

        let mut heal = before.clone();
        heal.pv.insert("u".into(), 12);
        let events = diff(&before, &heal, &session.state, &session.registry, None);
        assert_eq!(events[0].kind, VfxKind::Heal);
        assert_eq!(events[0].value, Some(2));
        assert_eq!(events[0].shake, None);
        assert_eq!(events[0].flash, Some(Flash::Heal));
    }

    #[test]
    fn the_last_pending_attack_colours_the_impact() {
        let session = session(7);
        let mut before = Snapshot::default();
        before.pv.insert("u".into(), 10);
        before.board.insert("u".into());
        let mut after = before.clone();
        after.pv.insert("u".into(), 6);

        let mut pending = pending("a", "u", false);
        pending.element = Some(Element::Fire);
        pending.is_special = true;
        pending.attack_traits.push(AttackTrait::Impact);

        let events = diff(
            &before,
            &after,
            &session.state,
            &session.registry,
            Some(&pending),
        );
        assert_eq!(events[0].element, VfxElement::Fire);
        assert!(events[0].impact, "impact attacks knock the tile back");
        assert_eq!(events[0].flash, Some(Flash::Knockback));
        assert!(events[0].big, "a special is always big");
        assert_eq!(events[0].shake, Some(Shake::Hard));
    }

    #[test]
    fn leaving_the_board_is_a_ko() {
        let session = session(7);
        let mut before = Snapshot::default();
        before.pv.insert("u".into(), 2);
        before.board.insert("u".into());
        let after = Snapshot::default();
        let events = diff(&before, &after, &session.state, &session.registry, None);
        assert_eq!(events.len(), 1, "no PV event for a card that vanished");
        assert_eq!(events[0].kind, VfxKind::Ko);
        assert_eq!(events[0].to_id.as_deref(), Some("u"));
    }

    #[test]
    fn a_captain_target_resolves_to_the_opposing_captain_key() {
        let session = session(7);
        let attacker = captain_key(PlayerId::Player1);
        let pending = pending(&attacker, "whatever", true);
        assert_eq!(
            attack_target_id(&pending, &session.state),
            captain_key(PlayerId::Player2)
        );
    }

    #[test]
    fn a_declared_attack_emits_a_projectile_with_a_lunge() {
        let mut session = session(7);
        let before = snapshot(&session.state);
        assert!(
            advance(&mut session, 400, |s| s.state.pending_attack.is_some()),
            "an attack must be declared within 400 actions"
        );
        let after = snapshot(&session.state);
        let events = diff(&before, &after, &session.state, &session.registry, None);
        let attack = events
            .iter()
            .find(|e| e.kind == VfxKind::Attack)
            .expect("the pending attack becomes a projectile");
        assert!(attack.from_id.is_some() && attack.to_id.is_some());
        assert_eq!(attack.flash, Some(Flash::Lunge));
        assert_eq!(attack.flash_target(), attack.from_id.as_deref());
        // A banner (and a cut-in) only for a named special / captain attack.
        assert_eq!(attack.wants_cut_in(), attack.big && attack.attacker_name.is_some());
    }
}

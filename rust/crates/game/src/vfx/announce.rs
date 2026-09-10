//! Play announcements — a port of `src/lib/announce.ts`.
//!
//! Every play, on **both** sides, is revealed at the centre of the screen: the
//! card pops in, holds, then flies toward the tile it belongs to (or fades, for
//! a text-only toast). Pure logic lives here; the reveal layer in
//! [`crate::vfx`] only draws what [`build_announcement`] returned and holds it
//! for [`announce_duration`].
//!
//! Like the TS, the announcement is built from the state **before** the action
//! is applied (`useGameEngine`'s `dispatch` announces first, executes second),
//! which is why [`crate::vfx`] keeps the previous [`GameState`] around.

use core::time::Duration;

use tcgop_engine::registry::CardRegistry;
use tcgop_engine::state::GameState;
use tcgop_engine::types::{
    AtkDefStat, CardDef, CounterEffect, DamageTarget, EventEffect, GameAction, HakiType, PlayerId,
    Slot,
};

use crate::ai_driver::{
    REVEAL_BIG, REVEAL_CAPTAIN, REVEAL_COMPACT, REVEAL_END_TURN, REVEAL_SPECIAL, REVEAL_TOAST,
};
use crate::selection::captain_key;

/// Who played — the reveal frame is green for you, red for the opponent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    You,
    Foe,
}

impl Side {
    /// TS `SIDE[side].label`.
    pub const fn label(self) -> &'static str {
        match self {
            Side::You => "Vous",
            Side::Foe => "\u{25C6} Adversaire",
        }
    }
}

/// TS `PlayAnnouncement`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayAnnouncement {
    pub side: Side,
    /// `None` → text-only toast.
    pub def_id: Option<String>,
    pub instance_id: Option<String>,
    /// `action.type_name()`.
    pub kind: &'static str,
    /// Engine id the card flies toward once the hold is over.
    pub dest_id: Option<String>,
    pub caption: String,
    /// Full card (`true`) vs compact card (`false`).
    pub big: bool,
    /// No card at all, just a banner.
    pub toast: bool,
}

impl PlayAnnouncement {
    fn base(side: Side, kind: &'static str, caption: String) -> Self {
        PlayAnnouncement {
            side,
            def_id: None,
            instance_id: None,
            kind,
            dest_id: None,
            caption,
            big: true,
            toast: false,
        }
    }

    /// How long the reveal stays on screen — TS `announceDuration`.
    pub fn duration(&self) -> Duration {
        announce_duration(self)
    }

    /// When the card starts flying / fading, as a fraction of
    /// [`PlayAnnouncement::duration`] (TS `holdMs`).
    pub fn hold_fraction(&self) -> f32 {
        if self.toast { 0.72 } else { 0.5 }
    }
}

/// TS `announceDuration(a)`.
pub fn announce_duration(announcement: &PlayAnnouncement) -> Duration {
    match announcement.kind {
        "specialAttack" | "fruitSpecialAttack" => REVEAL_SPECIAL,
        "captainAttack" => REVEAL_CAPTAIN,
        _ if announcement.toast => {
            if announcement.kind == "endTurn" {
                REVEAL_END_TURN
            } else {
                REVEAL_TOAST
            }
        }
        _ if !announcement.big => REVEAL_COMPACT,
        _ => REVEAL_BIG,
    }
}

/// TS `actorOf(action, state)` — during a counter window the answering player,
/// otherwise whoever is playing.
pub fn actor_of(action: &GameAction, state: &GameState) -> PlayerId {
    let current = state.current_player;
    if state.pending_attack.is_some() {
        match action {
            GameAction::PlayCounter { .. }
            | GameAction::UseShield { .. }
            | GameAction::PassCounter => return current.opponent(),
            GameAction::UseHaki {
                haki_type: HakiType::Observation,
                ..
            } => return current.opponent(),
            _ => {}
        }
    }
    current
}

fn def_of<'a>(
    state: &GameState,
    registry: &'a CardRegistry,
    instance_id: &str,
) -> Option<&'a CardDef> {
    state
        .cards
        .get(instance_id)
        .and_then(|card| registry.card_def(&card.def_id))
}

const fn slot_code(slot: Slot) -> &'static str {
    match slot {
        Slot::V1 => "V1",
        Slot::V2 => "V2",
        Slot::V3 => "V3",
        Slot::A1 => "A1",
        Slot::A2 => "A2",
        Slot::A3 => "A3",
    }
}

const fn stat_label(stat: AtkDefStat) -> &'static str {
    match stat {
        AtkDefStat::Atk => "ATK",
        AtkDefStat::Def => "DEF",
    }
}

const fn damage_target_label(target: DamageTarget) -> &'static str {
    match target {
        DamageTarget::AllFront => "allFront",
        DamageTarget::AllCursed => "allCursed",
        DamageTarget::All => "all",
        DamageTarget::Single => "single",
    }
}

/// TS `describeEvent(e)`.
pub fn describe_event(effect: &EventEffect) -> String {
    match effect {
        EventEffect::GainWill { amount } => format!("Gagne +{amount} Volonté."),
        EventEffect::Draw { amount, discard } => match discard {
            Some(discard) => format!("Pioche {amount}, défausse {discard}."),
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
        EventEffect::Rally { .. } => "Ralliement : +ATK/+DEF et soin.".to_string(),
        EventEffect::BuffSingle { stat, amount, .. } => {
            format!("Un allié +{amount} {}.", stat_label(*stat))
        }
        EventEffect::RushBuff { atk } => format!("Un allié gagne Rush +{atk} ATK."),
        EventEffect::DamageEnemies { amount, target, .. } => {
            format!("{amount} dégâts ({}).", damage_target_label(*target))
        }
        EventEffect::Tutor { .. } => "Cherche un personnage.".to_string(),
        EventEffect::DeployTokens { count, .. } => format!("Déploie {count} jeton(s)."),
        EventEffect::HealAllBuff { heal, atk } => format!("Soigne {heal} et +{atk} ATK."),
        EventEffect::DebuffAllEnemies { atk, .. } => format!("Ennemis -{atk} ATK."),
        EventEffect::GrantHakiAll { .. } => "Vos attaques gagnent le Haki.".to_string(),
        EventEffect::DodgeAll => "Esquive totale ce tour.".to_string(),
        EventEffect::Custom { description, .. } => description.clone(),
    }
}

/// TS `describeCounter(ce)`.
pub fn describe_counter(effect: &CounterEffect) -> String {
    match effect {
        CounterEffect::ReduceDamage { amount, .. } => {
            format!("Réduit les dégâts de {amount}.")
        }
        CounterEffect::Survive { .. } => "Survit avec 1 PV.".to_string(),
        CounterEffect::Cancel { .. } => "Annule l'attaque.".to_string(),
        CounterEffect::Untargetable { .. } => "Devient inciblable.".to_string(),
    }
}

/// TS `buildAnnouncement(action, state, humanPlayer)` — `state` is the state
/// **before** `action` was applied. `None` means "do not reveal this".
pub fn build_announcement(
    action: &GameAction,
    state: &GameState,
    registry: &CardRegistry,
    human: PlayerId,
) -> Option<PlayAnnouncement> {
    let actor = actor_of(action, state);
    let side = if actor == human { Side::You } else { Side::Foe };
    let kind = action.type_name();

    match action {
        GameAction::DeployCharacter { instance_id, slot } => {
            let def = def_of(state, registry, instance_id)?;
            let mut ann = PlayAnnouncement::base(
                side,
                kind,
                format!("Déploie {} en {}", def.name, slot_code(*slot)),
            );
            ann.def_id = Some(def.id.clone());
            ann.instance_id = Some(instance_id.clone());
            ann.dest_id = Some(instance_id.clone());
            Some(ann)
        }
        GameAction::DeployShip { instance_id } => {
            let def = def_of(state, registry, instance_id)?;
            let mut ann =
                PlayAnnouncement::base(side, kind, format!("Déploie le navire {}", def.name));
            ann.def_id = Some(def.id.clone());
            ann.instance_id = Some(instance_id.clone());
            Some(ann)
        }
        GameAction::EquipObject {
            object_instance_id,
            target_instance_id,
        } => {
            let def = def_of(state, registry, object_instance_id)?;
            let mut ann = PlayAnnouncement::base(side, kind, format!("Équipe {}", def.name));
            ann.def_id = Some(def.id.clone());
            ann.instance_id = Some(object_instance_id.clone());
            ann.dest_id = Some(target_instance_id.clone());
            Some(ann)
        }
        GameAction::PlayEvent { instance_id, .. } => {
            let def = def_of(state, registry, instance_id)?;
            let caption = match def.event_effect.as_ref() {
                Some(effect) => describe_event(effect),
                None => def.name.clone(),
            };
            let mut ann = PlayAnnouncement::base(side, kind, caption);
            ann.def_id = Some(def.id.clone());
            ann.instance_id = Some(instance_id.clone());
            Some(ann)
        }
        GameAction::PlayCounter { instance_id } => {
            let def = def_of(state, registry, instance_id)?;
            let caption = match def.counter_effect.as_ref() {
                Some(effect) => describe_counter(effect),
                None => def.name.clone(),
            };
            let mut ann = PlayAnnouncement::base(side, kind, caption);
            ann.def_id = Some(def.id.clone());
            ann.instance_id = Some(instance_id.clone());
            Some(ann)
        }
        GameAction::UseShield {
            blocker_instance_id,
        } => {
            let def = def_of(state, registry, blocker_instance_id)?;
            let mut ann =
                PlayAnnouncement::base(side, kind, format!("Bloque avec {} (Bouclier)", def.name));
            ann.def_id = Some(def.id.clone());
            ann.instance_id = Some(blocker_instance_id.clone());
            ann.dest_id = Some(blocker_instance_id.clone());
            Some(ann)
        }
        GameAction::ActivateShip { ship_instance_id } => {
            let def = def_of(state, registry, ship_instance_id)?;
            let name = def
                .ship_active
                .as_ref()
                .map(|active| active.name.clone())
                .unwrap_or_else(|| def.name.clone());
            let mut ann = PlayAnnouncement::base(side, kind, format!("Active {name}"));
            ann.def_id = Some(def.id.clone());
            ann.instance_id = Some(ship_instance_id.clone());
            Some(ann)
        }
        GameAction::AwakenFruit { fruit_instance_id } => {
            let def = def_of(state, registry, fruit_instance_id)?;
            let mut ann = PlayAnnouncement::base(side, kind, format!("Éveille {} !", def.name));
            ann.def_id = Some(def.id.clone());
            ann.instance_id = Some(fruit_instance_id.clone());
            Some(ann)
        }
        GameAction::BaseAttack {
            attacker_instance_id,
            ..
        } => {
            let def = def_of(state, registry, attacker_instance_id)?;
            let mut ann = PlayAnnouncement::base(side, kind, format!("{} attaque", def.name));
            ann.big = false;
            ann.def_id = Some(def.id.clone());
            ann.instance_id = Some(attacker_instance_id.clone());
            ann.dest_id = Some(attacker_instance_id.clone());
            Some(ann)
        }
        GameAction::SpecialAttack {
            attacker_instance_id,
            ..
        } => {
            let def = def_of(state, registry, attacker_instance_id)?;
            let name = def
                .special_attack
                .as_ref()
                .map(|special| special.name.clone())
                .unwrap_or_else(|| def.name.clone());
            let mut ann = PlayAnnouncement::base(side, kind, format!("Spéciale : {name}"));
            ann.big = false;
            ann.def_id = Some(def.id.clone());
            ann.instance_id = Some(attacker_instance_id.clone());
            ann.dest_id = Some(attacker_instance_id.clone());
            Some(ann)
        }
        GameAction::FruitSpecialAttack {
            attacker_instance_id,
            fruit_instance_id,
            ..
        } => {
            let def = def_of(state, registry, fruit_instance_id)?;
            let name = def
                .fruit_effects
                .as_ref()
                .and_then(|effects| effects.awakening.as_ref())
                .and_then(|awakening| awakening.special_attack.as_ref())
                .map(|special| special.name.clone())
                .unwrap_or_else(|| def.name.clone());
            let mut ann = PlayAnnouncement::base(side, kind, format!("Éveil : {name}"));
            ann.big = false;
            ann.def_id = Some(def.id.clone());
            ann.instance_id = Some(fruit_instance_id.clone());
            ann.dest_id = Some(attacker_instance_id.clone());
            Some(ann)
        }
        GameAction::MoveCharacter { instance_id, .. } => {
            let def = def_of(state, registry, instance_id)?;
            let mut ann = PlayAnnouncement::base(side, kind, format!("Déplace {}", def.name));
            ann.big = false;
            ann.def_id = Some(def.id.clone());
            ann.instance_id = Some(instance_id.clone());
            ann.dest_id = Some(instance_id.clone());
            Some(ann)
        }
        GameAction::FlipCaptain { .. } => {
            let mut ann =
                PlayAnnouncement::base(side, kind, "Retourne son Capitaine !".to_string());
            ann.toast = true;
            ann.dest_id = Some(captain_key(actor));
            Some(ann)
        }
        GameAction::CaptainAttack { .. } => {
            let mut ann = PlayAnnouncement::base(side, kind, "Le Capitaine attaque".to_string());
            ann.big = false;
            ann.toast = true;
            ann.dest_id = Some(captain_key(actor));
            Some(ann)
        }
        // Engine §8.2 item 34(b): the captain's surcharge ability.
        GameAction::UseSurcharge { .. } => {
            let mut ann =
                PlayAnnouncement::base(side, kind, "Le Capitaine utilise sa Surcharge".to_string());
            ann.big = false;
            ann.toast = true;
            ann.dest_id = Some(captain_key(actor));
            Some(ann)
        }
        GameAction::UseHaki { haki_type, .. } => {
            let caption = match haki_type {
                HakiType::King => "Haki des Rois !",
                HakiType::Observation => "Esquive (Haki Observation)",
                HakiType::Armament => "Haki",
            };
            let mut ann = PlayAnnouncement::base(side, kind, caption.to_string());
            ann.toast = true;
            Some(ann)
        }
        GameAction::PassCounter => {
            let caption = match side {
                Side::Foe => "L'adversaire encaisse",
                Side::You => "Vous encaissez",
            };
            let mut ann = PlayAnnouncement::base(side, kind, caption.to_string());
            ann.big = false;
            ann.toast = true;
            Some(ann)
        }
        GameAction::EndTurn => {
            let mut ann = PlayAnnouncement::base(side, kind, "Fin de tour".to_string());
            ann.big = false;
            ann.toast = true;
            Some(ann)
        }
        // Engine §8.1 item 59: the TS `default: return null` swallowed the
        // whole support-action class, so it never paced the loop. A support
        // action is announced as a short toast naming the card and its action.
        GameAction::BaseSupportAction { instance_id, .. } => {
            let def = def_of(state, registry, instance_id)?;
            let caption = match def.base_action.as_ref() {
                Some(base_action) => format!("{} : {}", def.name, base_action.name),
                None => def.name.clone(),
            };
            let mut ann = PlayAnnouncement::base(side, kind, caption);
            ann.big = false;
            ann.toast = true;
            ann.def_id = Some(def.id.clone());
            ann.instance_id = Some(instance_id.clone());
            ann.dest_id = Some(instance_id.clone());
            Some(ann)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::testkit::{advance, session};

    #[test]
    fn ending_the_turn_is_the_shortest_toast() {
        let session = session(3);
        let ann = build_announcement(
            &GameAction::EndTurn,
            &session.state,
            &session.registry,
            session.human,
        )
        .expect("end of turn is announced");
        assert_eq!(ann.side, Side::You, "player1 opens the game");
        assert!(ann.toast && !ann.big);
        assert_eq!(ann.caption, "Fin de tour");
        assert_eq!(ann.duration(), REVEAL_END_TURN);
        assert!(ann.dest_id.is_none());
    }

    #[test]
    fn the_ai_side_is_marked_as_the_foe() {
        let mut session = session(3);
        session.dispatch(GameAction::EndTurn).unwrap();
        let ann = build_announcement(
            &GameAction::EndTurn,
            &session.state,
            &session.registry,
            session.human,
        )
        .unwrap();
        assert_eq!(ann.side, Side::Foe);
        assert_eq!(Side::Foe.label(), "\u{25C6} Adversaire");
    }

    #[test]
    fn a_counter_answer_is_attributed_to_the_defender() {
        let mut session = session(3);
        assert!(
            advance(&mut session, 400, |s| s.state.pending_attack.is_some()),
            "an attack must be declared"
        );
        let attacker = session.state.current_player;
        let ann = build_announcement(
            &GameAction::PassCounter,
            &session.state,
            &session.registry,
            session.human,
        )
        .unwrap();
        let expected = if attacker.opponent() == session.human {
            Side::You
        } else {
            Side::Foe
        };
        assert_eq!(ann.side, expected, "the defender answers, not the attacker");
        assert_eq!(ann.duration(), REVEAL_TOAST);
    }

    #[test]
    fn a_deploy_names_the_card_and_flies_to_its_tile() {
        let mut session = session(11);
        assert!(
            advance(&mut session, 200, |s| s
                .state
                .cards
                .values()
                .any(|c| c.zone == tcgop_engine::types::Zone::Board)),
            "a character must reach the board"
        );
        let (id, card) = session
            .state
            .cards
            .iter()
            .find(|(_, c)| c.zone == tcgop_engine::types::Zone::Board)
            .unwrap();
        let action = GameAction::DeployCharacter {
            instance_id: id.clone(),
            slot: card.slot.unwrap_or(Slot::V1),
        };
        let ann =
            build_announcement(&action, &session.state, &session.registry, session.human).unwrap();
        assert!(ann.big && !ann.toast);
        assert_eq!(ann.dest_id.as_deref(), Some(id.as_str()));
        assert_eq!(ann.def_id.as_deref(), Some(card.def_id.as_str()));
        assert!(ann.caption.starts_with("Déploie "));
        assert_eq!(ann.duration(), REVEAL_BIG);
    }

    #[test]
    fn attacks_hold_the_screen_the_longest() {
        let session = session(3);
        let mut special = PlayAnnouncement::base(Side::You, "specialAttack", String::new());
        special.big = false;
        assert_eq!(special.duration(), REVEAL_SPECIAL);

        let mut captain = PlayAnnouncement::base(Side::Foe, "captainAttack", String::new());
        captain.toast = true;
        captain.big = false;
        assert_eq!(captain.duration(), REVEAL_CAPTAIN);

        let mut compact = PlayAnnouncement::base(Side::You, "baseAttack", String::new());
        compact.big = false;
        assert_eq!(compact.duration(), REVEAL_COMPACT);
        assert_eq!(compact.hold_fraction(), 0.5);
        assert_eq!(captain.hold_fraction(), 0.72);

        // An unknown instance is still unannounceable (nothing to name).
        assert!(
            build_announcement(
                &GameAction::BaseSupportAction {
                    instance_id: "x".into(),
                    target_instance_id: None,
                },
                &session.state,
                &session.registry,
                session.human,
            )
            .is_none()
        );
    }

    /// Any card in the game whose base action is a support action.
    fn a_support_card(session: &crate::bridge::Session) -> Vec<String> {
        let mut ids: Vec<String> = session
            .state
            .cards
            .iter()
            .filter(|(_, card)| {
                session
                    .registry
                    .card_def(&card.def_id)
                    .and_then(|def| def.base_action.as_ref())
                    .is_some_and(|base| base.is_support.unwrap_or(false))
            })
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort();
        ids
    }

    /// §8.1 item 59 — the TS `default: return null` swallowed the whole
    /// support-action class, so it never paced the loop.
    #[test]
    fn a_support_action_is_announced_as_a_short_toast() {
        let session = session(3);
        let ids = a_support_card(&session);
        let id = ids.first().expect("both decks hold a support action");
        let def = session
            .registry
            .card_def(&session.state.cards[id].def_id)
            .unwrap();

        let ann = build_announcement(
            &GameAction::BaseSupportAction {
                instance_id: id.clone(),
                target_instance_id: None,
            },
            &session.state,
            &session.registry,
            session.human,
        )
        .expect("a support action paces the loop like every other action");

        assert!(ann.toast && !ann.big, "a short banner, not a card reveal");
        assert_eq!(ann.kind, "baseSupportAction");
        assert_eq!(
            ann.caption,
            format!("{} : {}", def.name, def.base_action.as_ref().unwrap().name)
        );
        assert_eq!(ann.duration(), REVEAL_TOAST);
        assert_eq!(ann.instance_id.as_deref(), Some(id.as_str()));
        assert_eq!(ann.def_id.as_deref(), Some(def.id.as_str()));
    }

    /// The point of the toast: two support actions in a row queue up instead of
    /// firing back to back with no pause at all.
    #[test]
    fn two_support_actions_in_a_row_do_not_overlap_their_reveals() {
        use crate::ai_driver::AiPacing;

        let session = session(3);
        let ids = a_support_card(&session);
        assert!(ids.len() >= 2, "need two support cards to chain them");

        let mut pacing = AiPacing::default();
        let now = Duration::from_secs(2);
        for id in ids.iter().take(2) {
            let ann = build_announcement(
                &GameAction::BaseSupportAction {
                    instance_id: id.clone(),
                    target_instance_id: None,
                },
                &session.state,
                &session.registry,
                session.human,
            )
            .expect("every support action is announced");
            pacing.extend(now, ann.duration());
        }
        assert_eq!(
            pacing.remaining_busy(now),
            REVEAL_TOAST + REVEAL_TOAST,
            "the second reveal starts where the first one ends"
        );
    }

    #[test]
    fn haki_and_flip_are_toasts_aimed_at_the_captain() {
        let session = session(3);
        let flip = build_announcement(
            &GameAction::FlipCaptain { slot: Slot::V2 },
            &session.state,
            &session.registry,
            session.human,
        )
        .unwrap();
        assert!(flip.toast);
        assert_eq!(flip.dest_id, Some(captain_key(session.human)));

        let haki = build_announcement(
            &GameAction::UseHaki {
                haki_type: HakiType::King,
                target_instance_id: None,
            },
            &session.state,
            &session.registry,
            session.human,
        )
        .unwrap();
        assert!(haki.toast && haki.dest_id.is_none());
        assert_eq!(haki.caption, "Haki des Rois !");
    }
}

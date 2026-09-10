//! Haki — port of `src/engine/haki.ts`.
//!
//! Three Haki types unlock on fixed turn thresholds. Observation (T5+) is a
//! once-per-turn dodge played in the counter window; Armament (T7+) is a pure
//! passive in Rulebook v3.1 and is therefore **never** available as an action;
//! King / Roi (T10+) is a once-per-game board wipe of every enemy with
//! effective DEF ≤ 3, gated on controlling a Conquerant unit.

use crate::board::{get_board_characters, get_effective_def, remove_from_board};
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::passives::apply_on_ko_effects;
use crate::registry::CardRegistry;
use crate::state::GameState;
use crate::types::{HakiType, PlayerId, Trait};

/// TS `HAKI_THRESHOLDS` — `src/engine/haki.ts:9`
/// (`{ observation: 5, armament: 7, king: 10 }`).
pub const HAKI_THRESHOLDS: [(HakiType, u32); 3] = [
    (HakiType::Observation, 5),
    (HakiType::Armament, 7),
    (HakiType::King, 10),
];

/// The turn from which `haki_type` is unlocked (TS `HAKI_THRESHOLDS[hakiType]`).
pub fn haki_threshold(haki_type: HakiType) -> u32 {
    match haki_type {
        HakiType::Observation => 5,
        HakiType::Armament => 7,
        HakiType::King => 10,
    }
}

/// TS `isHakiAvailable(state, playerId, hakiType)` — `src/engine/haki.ts:16`.
///
/// `false` below the turn threshold; then `!observationUsed` for Observation,
/// **always `false`** for Armament (a passive since v3.1), `!kingUsed` for King.
pub fn is_haki_available(state: &GameState, player_id: PlayerId, haki_type: HakiType) -> bool {
    let player = state.players.get(player_id);
    if state.turn_number < haki_threshold(haki_type) {
        return false;
    }

    match haki_type {
        HakiType::Observation => !player.observation_used,
        // Armament is a passive from T7+ (Rulebook v3.1 §7) — never "used".
        HakiType::Armament => false,
        HakiType::King => !player.king_used,
    }
}

/// TS `useObservationHaki(state, playerId)` — `src/engine/haki.ts:36`.
///
/// Sets `observationUsed`, clears `pendingAttack` and logs
/// `"Haki de l'Observation ! Attaque esquivee !"`.
///
/// Errors: `Observation Haki not available`, `No pending attack to dodge`,
/// `This attack cannot be dodged`.
pub fn use_observation_haki(state: &mut GameState, player_id: PlayerId) -> Result<(), EngineError> {
    if !is_haki_available(state, player_id, HakiType::Observation) {
        return Err(EngineError::illegal("Observation Haki not available"));
    }
    let Some(pending) = state.pending_attack.as_ref() else {
        return Err(EngineError::illegal("No pending attack to dodge"));
    };
    if pending.cannot_be_dodged.unwrap_or(false) {
        return Err(EngineError::illegal("This attack cannot be dodged"));
    }

    state.players.get_mut(player_id).observation_used = true;
    // Cancel the pending attack
    state.pending_attack = None;

    state.add_log(player_id, "Haki de l'Observation ! Attaque esquivee !");
    Ok(())
}

/// TS `hasConquerorInPlay(state, playerId)` — `src/engine/haki.ts:58`.
///
/// True when the captain's current face (verso traits when flipped, otherwise
/// the `CaptainDef.traits`) carries `conqueror`, when `CaptainDef.traits` does,
/// or when any board character's def does.
pub fn has_conqueror_in_play(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Result<bool, EngineError> {
    let captain = &state.players.get(player_id).captain;
    let cap_def = registry.get_captain_def(&captain.def_id)?;
    // NB (TS quirk): the recto side reads `capDef.traits`, not `capDef.recto.traits`.
    let cap_traits = if captain.flipped {
        cap_def.verso.traits.as_ref()
    } else {
        cap_def.traits.as_ref()
    };
    if cap_traits.is_some_and(|ts| ts.contains(&Trait::Conqueror)) {
        return Ok(true);
    }
    if cap_def
        .traits
        .as_ref()
        .is_some_and(|ts| ts.contains(&Trait::Conqueror))
    {
        return Ok(true);
    }

    // `Array.prototype.some` short-circuits, so a later unknown def id is never
    // looked up once an earlier character already matched.
    let def_ids: Vec<String> = get_board_characters(state, player_id)
        .iter()
        .map(|c| c.def_id.clone())
        .collect();
    for def_id in def_ids {
        if registry.get_card_def(&def_id)?.has_trait(Trait::Conqueror) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// TS `useKingHaki(state, playerId)` — `src/engine/haki.ts:75`.
///
/// Sets `kingUsed`, logs `"👑 Haki des Rois ! Tous les ennemis DEF ≤ 3 sont KO !"`,
/// then snapshots the victims (effective DEF ≤ 3, board order) **before**
/// removing any, and for each logs `"{name} est KO (Haki des Rois) !"`,
/// grants the +2 Vol KO bonus to the victim's owner, removes it and runs
/// `apply_on_ko_effects(victimOwner, playerId, victimDefId)`.
///
/// Errors: `Roi Haki not available`,
/// `Roi Haki requires a Conquerant unit in play`.
pub fn use_king_haki(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    player_id: PlayerId,
) -> Result<(), EngineError> {
    if !is_haki_available(state, player_id, HakiType::King) {
        return Err(EngineError::illegal("Roi Haki not available"));
    }
    if !has_conqueror_in_play(state, registry, player_id)? {
        return Err(EngineError::illegal(
            "Roi Haki requires a Conquerant unit in play",
        ));
    }

    let opponent_id = player_id.opponent();
    state.players.get_mut(player_id).king_used = true;
    state.add_log(
        player_id,
        "👑 Haki des Rois ! Tous les ennemis DEF ≤ 3 sont KO !",
    );

    // Snapshot victims first (board mutates as we remove them).
    let candidates: Vec<(String, String, PlayerId)> = get_board_characters(state, opponent_id)
        .iter()
        .map(|c| (c.instance_id.clone(), c.def_id.clone(), c.owner))
        .collect();
    let mut victims: Vec<(String, String, PlayerId)> = Vec::new();
    for (instance_id, def_id, owner) in candidates {
        if get_effective_def(state, registry, &instance_id)? <= 3 {
            victims.push((instance_id, def_id, owner));
        }
    }

    for (id, def_id, owner) in victims {
        match state.cards.get(&id) {
            Some(card) if card.zone == crate::types::Zone::Board => {}
            _ => continue,
        }
        let name = registry.get_card_def(&def_id)?.name.clone();
        state.add_log(owner, format!("{name} est KO (Haki des Rois) !"));
        state.grant_ko_bonus(owner);
        remove_from_board(state, registry, &id)?;
        apply_on_ko_effects(state, registry, ctx, owner, player_id, &def_id)?;
    }

    Ok(())
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Board, CaptainInstance, CardInstance, PendingAttack, PlayerState, Players};
    use crate::types::{
        BaseAction, CaptainRecto, CaptainVerso, CardDef, CardType, EntryEffect, Faction,
        FlipCondition, PassiveDef, Phase, Rarity, Slot, SpecialAttack, Zone,
    };
    use std::collections::BTreeMap;

    fn passive() -> PassiveDef {
        PassiveDef {
            name: "p".into(),
            description: "d".into(),
            effects: Vec::new(),
        }
    }

    /// A captain whose recto (`CaptainDef.traits`) and verso traits are set
    /// independently, so the two TS trait checks can be told apart.
    fn captain_def(
        id: &str,
        top_traits: Option<Vec<Trait>>,
        verso_traits: Option<Vec<Trait>>,
    ) -> crate::types::CaptainDef {
        crate::types::CaptainDef {
            id: id.into(),
            name: format!("Cap {id}"),
            faction: Faction::Pirate,
            tags: None,
            traits: top_traits,
            recto: CaptainRecto {
                pv: 20,
                atk: 3,
                def: 2,
                passive: passive(),
                attacks: Vec::new(),
                surcharge: None,
            },
            flip_condition: FlipCondition::default(),
            verso: CaptainVerso {
                pv: 25,
                atk: 5,
                def: 3,
                passive: passive(),
                entry_effect: EntryEffect::GrantSelfRush,
                base_action: BaseAction {
                    name: "b".into(),
                    atk: 3,
                    ..Default::default()
                },
                special_attack: SpecialAttack {
                    name: "s".into(),
                    cost: 1,
                    atk_bonus: 1,
                    ..Default::default()
                },
                surcharge: None,
                traits: verso_traits,
                natural_haki: None,
            },
        }
    }

    fn char_def(id: &str, def_value: i32, traits: Option<Vec<Trait>>) -> CardDef {
        let mut d = CardDef::new(
            id,
            format!("Perso {id}"),
            CardType::Character,
            2,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        d.atk = Some(2);
        d.def = Some(def_value);
        d.pv = Some(5);
        d.traits = traits;
        d
    }

    fn player(id: PlayerId, captain_def_id: &str) -> PlayerState {
        PlayerState {
            id,
            captain: CaptainInstance::new(captain_def_id.to_string(), id, 20),
            deck: Vec::new(),
            hand: Vec::new(),
            graveyard: Vec::new(),
            board: Board::empty(),
            active_ship: None,
            volonte: 0,
            used_free_move: false,
            has_drawn: false,
            observation_used: false,
            armament_used: false,
            king_used: false,
            ally_ko_ed_this_turn: None,
            char_ko_ed_this_game: None,
            haki_this_turn: None,
        }
    }

    fn state_with(turn: u32) -> GameState {
        GameState {
            cards: BTreeMap::new(),
            players: Players {
                player1: player(PlayerId::Player1, "CAP-P1"),
                player2: player(PlayerId::Player2, "CAP-P2"),
            },
            turn_number: turn,
            current_player: PlayerId::Player1,
            phase: Phase::Main,
            pending_attack: None,
            log: Vec::new(),
            winner: None,
            first_player: PlayerId::Player1,
        }
    }

    /// Put a character on `owner`'s board in `slot`.
    fn place(state: &mut GameState, owner: PlayerId, slot: Slot, instance_id: &str, def_id: &str) {
        let mut inst = CardInstance::new(instance_id.into(), def_id.into(), owner, 5);
        inst.zone = Zone::Board;
        inst.slot = Some(slot);
        state.cards.insert(instance_id.to_string(), inst);
        state
            .players
            .get_mut(owner)
            .board
            .set(slot, Some(instance_id.to_string()));
    }

    fn pending() -> PendingAttack {
        PendingAttack {
            attacker_id: "A".into(),
            target_id: "T".into(),
            target_is_captain: false,
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
        }
    }

    // ---------- isHakiAvailable ----------

    #[test]
    fn thresholds_match_the_ts_record() {
        assert_eq!(haki_threshold(HakiType::Observation), 5);
        assert_eq!(haki_threshold(HakiType::Armament), 7);
        assert_eq!(haki_threshold(HakiType::King), 10);
        assert_eq!(
            HAKI_THRESHOLDS,
            [
                (HakiType::Observation, 5),
                (HakiType::Armament, 7),
                (HakiType::King, 10)
            ]
        );
    }

    #[test]
    fn availability_is_gated_by_turn_then_by_used_flag() {
        let mut s = state_with(4);
        assert!(!is_haki_available(
            &s,
            PlayerId::Player1,
            HakiType::Observation
        ));
        s.turn_number = 5;
        assert!(is_haki_available(
            &s,
            PlayerId::Player1,
            HakiType::Observation
        ));
        s.players.get_mut(PlayerId::Player1).observation_used = true;
        assert!(!is_haki_available(
            &s,
            PlayerId::Player1,
            HakiType::Observation
        ));
        // …per player.
        assert!(is_haki_available(
            &s,
            PlayerId::Player2,
            HakiType::Observation
        ));

        s.turn_number = 9;
        assert!(!is_haki_available(&s, PlayerId::Player1, HakiType::King));
        s.turn_number = 10;
        assert!(is_haki_available(&s, PlayerId::Player1, HakiType::King));
        s.players.get_mut(PlayerId::Player1).king_used = true;
        assert!(!is_haki_available(&s, PlayerId::Player1, HakiType::King));
    }

    #[test]
    fn armament_is_never_available_even_unused_past_its_threshold() {
        let mut s = state_with(99);
        assert!(!s.players.get(PlayerId::Player1).armament_used);
        assert!(!is_haki_available(
            &s,
            PlayerId::Player1,
            HakiType::Armament
        ));
        s.players.get_mut(PlayerId::Player1).armament_used = true;
        assert!(!is_haki_available(
            &s,
            PlayerId::Player1,
            HakiType::Armament
        ));
    }

    // ---------- useObservationHaki ----------

    #[test]
    fn observation_dodge_cancels_the_attack_and_logs_verbatim() {
        let mut s = state_with(5);
        s.pending_attack = Some(pending());
        use_observation_haki(&mut s, PlayerId::Player2).unwrap();
        assert!(s.pending_attack.is_none());
        assert!(s.players.get(PlayerId::Player2).observation_used);
        assert_eq!(s.log.len(), 1);
        assert_eq!(
            s.log[0].message,
            "Haki de l'Observation ! Attaque esquivee !"
        );
        assert_eq!(s.log[0].player, PlayerId::Player2);
    }

    #[test]
    fn observation_dodge_error_order_matches_the_ts() {
        // 1. availability is checked before the pending attack exists.
        let mut s = state_with(4);
        assert_eq!(
            use_observation_haki(&mut s, PlayerId::Player2),
            Err(EngineError::illegal("Observation Haki not available"))
        );
        // 2. no pending attack.
        let mut s = state_with(5);
        assert_eq!(
            use_observation_haki(&mut s, PlayerId::Player2),
            Err(EngineError::illegal("No pending attack to dodge"))
        );
        // 3. undodgeable attack — nothing is mutated.
        let mut s = state_with(5);
        let mut p = pending();
        p.cannot_be_dodged = Some(true);
        s.pending_attack = Some(p);
        assert_eq!(
            use_observation_haki(&mut s, PlayerId::Player2),
            Err(EngineError::illegal("This attack cannot be dodged"))
        );
        assert!(s.pending_attack.is_some());
        assert!(!s.players.get(PlayerId::Player2).observation_used);
        assert!(s.log.is_empty());
    }

    // ---------- hasConquerorInPlay ----------

    #[test]
    fn conqueror_is_found_on_the_captain_top_level_traits() {
        let mut reg = CardRegistry::new();
        reg.register_captain(captain_def("CAP-P1", Some(vec![Trait::Conqueror]), None));
        reg.register_captain(captain_def("CAP-P2", None, None));
        let s = state_with(10);
        assert!(has_conqueror_in_play(&s, &reg, PlayerId::Player1).unwrap());
        assert!(!has_conqueror_in_play(&s, &reg, PlayerId::Player2).unwrap());
    }

    #[test]
    fn flipped_captain_reads_verso_traits_but_still_falls_back_to_top_level() {
        let mut reg = CardRegistry::new();
        // Verso-only conqueror: matches only once flipped.
        reg.register_captain(captain_def("CAP-P1", None, Some(vec![Trait::Conqueror])));
        // Top-level-only conqueror: the TS second check matches even when flipped.
        reg.register_captain(captain_def("CAP-P2", Some(vec![Trait::Conqueror]), None));

        let mut s = state_with(10);
        assert!(!has_conqueror_in_play(&s, &reg, PlayerId::Player1).unwrap());
        s.players.get_mut(PlayerId::Player1).captain.flipped = true;
        assert!(has_conqueror_in_play(&s, &reg, PlayerId::Player1).unwrap());

        s.players.get_mut(PlayerId::Player2).captain.flipped = true;
        assert!(has_conqueror_in_play(&s, &reg, PlayerId::Player2).unwrap());
    }

    #[test]
    fn conqueror_is_found_on_a_board_character() {
        let mut reg = CardRegistry::new();
        reg.register_captain(captain_def("CAP-P1", None, None));
        reg.register_captain(captain_def("CAP-P2", None, None));
        reg.register_card(char_def("C-PLAIN", 2, None));
        reg.register_card(char_def("C-CONQ", 2, Some(vec![Trait::Conqueror])));

        let mut s = state_with(10);
        place(&mut s, PlayerId::Player1, Slot::V1, "i1", "C-PLAIN");
        assert!(!has_conqueror_in_play(&s, &reg, PlayerId::Player1).unwrap());
        place(&mut s, PlayerId::Player1, Slot::A2, "i2", "C-CONQ");
        assert!(has_conqueror_in_play(&s, &reg, PlayerId::Player1).unwrap());
    }

    // ---------- useKingHaki ----------

    fn king_registry() -> CardRegistry {
        let mut reg = CardRegistry::new();
        reg.register_captain(captain_def("CAP-P1", Some(vec![Trait::Conqueror]), None));
        reg.register_captain(captain_def("CAP-P2", None, None));
        reg.register_card(char_def("C-DEF1", 1, None));
        reg.register_card(char_def("C-DEF3", 3, None));
        reg.register_card(char_def("C-DEF4", 4, None));
        reg
    }

    #[test]
    fn king_haki_kos_every_enemy_at_def_three_or_less() {
        let reg = king_registry();
        let ctx = EngineContext::seeded(1);
        let mut s = state_with(10);
        place(&mut s, PlayerId::Player2, Slot::V1, "e1", "C-DEF1");
        place(&mut s, PlayerId::Player2, Slot::V2, "e2", "C-DEF4");
        place(&mut s, PlayerId::Player2, Slot::A1, "e3", "C-DEF3");
        // Own characters are untouched.
        place(&mut s, PlayerId::Player1, Slot::V1, "a1", "C-DEF1");

        use_king_haki(&mut s, &reg, &ctx, PlayerId::Player1).unwrap();

        assert!(s.players.get(PlayerId::Player1).king_used);
        assert_eq!(s.cards["e1"].zone, Zone::Graveyard);
        assert_eq!(s.cards["e3"].zone, Zone::Graveyard);
        assert_eq!(s.cards["e2"].zone, Zone::Board);
        assert_eq!(s.cards["a1"].zone, Zone::Board);
        assert_eq!(
            s.players.get(PlayerId::Player2).board.get(Slot::V2),
            Some(&"e2".to_string())
        );
        assert_eq!(s.players.get(PlayerId::Player2).board.get(Slot::V1), None);

        // +2 Vol per lost ally, to the victim's owner.
        assert_eq!(s.players.get(PlayerId::Player2).volonte, 4);
        assert_eq!(s.players.get(PlayerId::Player1).volonte, 0);

        // Board order V1,V2,V3,A1,A2,A3 → e1 before e3.
        let messages: Vec<&str> = s.log.iter().map(|l| l.message.as_str()).collect();
        assert_eq!(
            messages[0],
            "👑 Haki des Rois ! Tous les ennemis DEF ≤ 3 sont KO !"
        );
        assert_eq!(messages[1], "Perso C-DEF1 est KO (Haki des Rois) !");
        assert!(messages.contains(&"Perso C-DEF3 est KO (Haki des Rois) !"));
        assert_eq!(s.log[0].player, PlayerId::Player1);
        assert_eq!(s.log[1].player, PlayerId::Player2);
    }

    #[test]
    fn king_haki_error_order_matches_the_ts() {
        let reg = king_registry();
        let ctx = EngineContext::seeded(1);

        // Before T10.
        let mut s = state_with(9);
        assert_eq!(
            use_king_haki(&mut s, &reg, &ctx, PlayerId::Player1),
            Err(EngineError::illegal("Roi Haki not available"))
        );
        // Already used.
        let mut s = state_with(10);
        s.players.get_mut(PlayerId::Player1).king_used = true;
        assert_eq!(
            use_king_haki(&mut s, &reg, &ctx, PlayerId::Player1),
            Err(EngineError::illegal("Roi Haki not available"))
        );
        // No Conquerant unit — and `kingUsed` is NOT consumed.
        let mut s = state_with(10);
        assert_eq!(
            use_king_haki(&mut s, &reg, &ctx, PlayerId::Player2),
            Err(EngineError::illegal(
                "Roi Haki requires a Conquerant unit in play"
            ))
        );
        assert!(!s.players.get(PlayerId::Player2).king_used);
        assert!(s.log.is_empty());
    }

    #[test]
    fn king_haki_with_no_victims_still_spends_the_use_and_logs() {
        let reg = king_registry();
        let ctx = EngineContext::seeded(1);
        let mut s = state_with(10);
        place(&mut s, PlayerId::Player2, Slot::V1, "e1", "C-DEF4");

        use_king_haki(&mut s, &reg, &ctx, PlayerId::Player1).unwrap();
        assert!(s.players.get(PlayerId::Player1).king_used);
        assert_eq!(s.cards["e1"].zone, Zone::Board);
        assert_eq!(s.log.len(), 1);
        assert_eq!(s.players.get(PlayerId::Player2).volonte, 0);
    }
}

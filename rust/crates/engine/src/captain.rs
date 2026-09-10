//! Captains — port of `src/engine/captain.ts`.
//!
//! A captain starts *recto*: off-board, unable to attack, contributing only its
//! passive. Flipping it (irreversibly) puts the *verso* face into a board slot,
//! carries the marked damage over (`versoPv - (rectoPv - currentPv)`), triggers
//! the verso `entryEffect` and unlocks the captain's base attack.

use crate::board::{get_board_characters, get_effective_atk, get_effective_def, remove_from_board};
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::passives::apply_on_ko_effects;
use crate::registry::CardRegistry;
use crate::state::{GameState, PendingAttack};
use crate::types::{
    AtkDefStat, EntryDamageTarget, EntryEffect, Modifier, ModifierDuration, ModifierStat,
    PassiveEffect, PlayerId, Slot, StatusEffect, StatusEffectType, Trait, Zone,
};

/// TS `canFlipCaptain(state, playerId)` — `src/engine/captain.ts:11`.
///
/// `false` once flipped. Then, in order: a free flip when
/// `freeIfAllyKO && allyKOedThisTurn`; a free flip when
/// `autoIfAlliesLte !== undefined && turnNumber >= 4 && allyCount <= autoIfAlliesLte`;
/// otherwise `canAfford(condition.cost)` when a cost exists, else `false`.
///
/// Note the TS engine never reads `freeIfEnemyCursed`, `freeIfAlliesGte` or
/// `freeIfTurnGte` — the port must ignore them too.
pub fn can_flip_captain(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Result<bool, EngineError> {
    let captain = &state.players.get(player_id).captain;
    if captain.flipped {
        // Already flipped (irreversible)
        return Ok(false);
    }

    let def = registry.get_captain_def(&captain.def_id)?;
    let condition = &def.flip_condition;

    // Free flip if a Mugiwara ally was KO'd this turn (Luffy).
    if condition.free_if_ally_ko.unwrap_or(false)
        && state.players.get(player_id).ally_koed_this_turn()
    {
        return Ok(true);
    }

    // Check auto-flip condition (allies <= N)
    // Only available from turn 4+ to prevent early abuse
    if let Some(max_allies) = condition.auto_if_allies_lte
        && state.turn_number >= 4
    {
        let ally_count = get_board_characters(state, player_id).len() as i32;
        if ally_count <= max_allies {
            return Ok(true);
        }
    }

    // Check Vol. cost
    if let Some(cost) = condition.cost {
        return Ok(state.can_afford(player_id, cost));
    }

    Ok(false)
}

/// TS `flipCaptain(state, playerId, slot)` — `src/engine/captain.ts:44`.
///
/// Recomputes the free-flip test, pays `condition.cost` otherwise, sets
/// `flipped`, `currentPv = verso.pv - max(0, recto.pv - currentPv)`, `slot`
/// and `deployedTurn`, logs
/// `"{name} s'engage sur le champ de bataille ! (verso, slot {slot})"`,
/// checks the win condition (flipping into a smaller face can be lethal — the
/// function returns **before** the entry effect in that case) and finally runs
/// [`resolve_entry_effect`].
///
/// Errors: `Captain already flipped`, `Slot {slot} is occupied`,
/// `Cannot afford captain flip`.
pub fn flip_captain(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    slot: Slot,
) -> Result<(), EngineError> {
    let captain = &state.players.get(player_id).captain;
    if captain.flipped {
        return Err(EngineError::illegal("Captain already flipped"));
    }

    let def = registry.get_captain_def(&captain.def_id)?.clone();
    let condition = &def.flip_condition;

    // Check slot availability
    if state.players.get(player_id).board.get(slot).is_some() {
        return Err(EngineError::SlotOccupied(slot));
    }

    // Determine cost
    let mut cost = 0;
    let ally_count = get_board_characters(state, player_id).len() as i32;
    let free_flip = (condition.free_if_ally_ko.unwrap_or(false)
        && state.players.get(player_id).ally_koed_this_turn())
        || (match condition.auto_if_allies_lte {
            Some(max_allies) => state.turn_number >= 4 && ally_count <= max_allies,
            None => false,
        });

    if !free_flip {
        cost = condition.cost.unwrap_or(0);
        if !state.can_afford(player_id, cost) {
            return Err(EngineError::illegal("Cannot afford captain flip"));
        }
    }

    if cost > 0 {
        state.spend_volonte(player_id, cost)?;
    }

    // Flip captain — damage already marked on the recto face carries over to the verso
    // face (Rulebook v3.1 §2.1: the face/PV max changes, marked damage stays).
    let turn_number = state.turn_number;
    {
        let cap = &mut state.players.get_mut(player_id).captain;
        cap.flipped = true;
        let dmg_marked = (def.recto.pv - cap.current_pv).max(0);
        cap.current_pv = def.verso.pv - dmg_marked;
        cap.slot = Some(slot);
        cap.deployed_turn = Some(i64::from(turn_number));
    }

    state.add_log(
        player_id,
        format!(
            "{} s'engage sur le champ de bataille ! (verso, slot {})",
            def.name,
            slot.as_str()
        ),
    );

    // Flipping a badly wounded captain into a lower-PV face can be lethal.
    if let Some(flip_winner) = state.check_win_condition() {
        state.winner = Some(flip_winner);
        return Ok(());
    }

    // Apply entry effect
    resolve_entry_effect(state, registry, ctx, player_id, &def.verso.entry_effect)
}

/// TS `resolveEntryEffect(state, playerId, effect)` — `src/engine/captain.ts:112`.
///
/// Recursive over `multi`. Per variant:
/// - `buffAllies` → a `turn` modifier `` `entry_{Date.now()}` `` (source = the
///   captain's `defId`) on every own board character + log
///   `"Effet d'entree : tous allies +{amount} {STAT} ce tour !"` (stat upper-cased);
/// - `draw` → N `drawCard` + `"Effet d'entree : pioche {n} carte(s)"`;
/// - `damageEnemies` `allFront` → `amount` to V1–V3 (no KO sweep!) +
///   `"Effet d'entree : {n} degats a toute la Ligne Avant ennemie !"`;
/// - `damageEnemies` `single` → strongest-ATK enemy in the front pool (else any),
///   `cursedBonus` replaces `amount` for Cursed targets, `sand` adds 1, log
///   `"Effet d'entree : {n} degats a {name} !"` and the full KO chain
///   (`"{name} est KO !"`); with no enemy character at all the captain takes
///   `amount` with `"Effet d'entree (Gear 2) : {n} degats au Capitaine !"`;
/// - `grantSelfRush` → `captain.deployedTurn = -1` +
///   `"Effet d'entree : Gear 2 — le Capitaine peut agir immédiatement (Rush)."`;
/// - `haoshoku` → every enemy gets a `turn` `-debuffAtk` modifier
///   `` `haoshoku_{instanceId}_{Date.now()}` `` and, unless `immuneControl`, an
///   `immobilize` 2t when the **printed** `def.def ?? 0 <= immobilizeMaxDef`;
///   log `"Effet d'entree : Haoshoku Haki !"`;
/// - `debuffAllEnemies` → `` `entrydebuff_{instanceId}_{Date.now()}` `` turn modifier, no log;
/// - `discardOpponentRandom` → N random discards from the opponent's hand
///   ([`EngineContext::rng`]), no log;
/// - `custom` → `"Effet d'entree special : {description}"`.
///
/// `deployedTurn = -1` in TS is a negative turn number, so
/// `CaptainInstance::deployed_turn` is an `Option<i64>` and the sentinel is
/// written verbatim — see the notes in `state.rs`.
pub fn resolve_entry_effect(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    effect: &EntryEffect,
) -> Result<(), EngineError> {
    match effect {
        EntryEffect::BuffAllies {
            stat,
            amount,
            duration: _,
        } => {
            let (mod_stat, stat_upper) = match stat {
                AtkDefStat::Atk => (ModifierStat::Atk, "ATK"),
                AtkDefStat::Def => (ModifierStat::Def, "DEF"),
            };
            let source = state.players.get(player_id).captain.def_id.clone();
            let mod_id = format!("entry_{}", ctx.now_ms);
            let ids: Vec<String> = state
                .players
                .get(player_id)
                .board
                .instance_ids()
                .into_iter()
                .cloned()
                .collect();
            for id in ids {
                if let Some(card) = state.cards.get_mut(&id) {
                    card.modifiers.push(Modifier {
                        id: mod_id.clone(),
                        stat: mod_stat,
                        amount: *amount,
                        source: source.clone(),
                        duration: ModifierDuration::Turn,
                        turns_remaining: None,
                    });
                }
            }
            state.add_log(
                player_id,
                format!("Effet d'entree : tous allies +{amount} {stat_upper} ce tour !"),
            );
        }

        EntryEffect::Draw { amount } => {
            for _ in 0..(*amount).max(0) {
                state.draw_card(player_id);
            }
            state.add_log(
                player_id,
                format!("Effet d'entree : pioche {amount} carte(s)"),
            );
        }

        EntryEffect::DamageEnemies {
            amount,
            target,
            cursed_bonus,
            sand,
        } => {
            let opponent_id = player_id.opponent();

            match target {
                EntryDamageTarget::AllFront => {
                    for slot_key in Slot::FRONT {
                        let id = state.players.get(opponent_id).board.get(slot_key).cloned();
                        if let Some(id) = id
                            && let Some(card) = state.cards.get_mut(&id)
                        {
                            card.current_pv -= *amount;
                        }
                    }
                    state.add_log(
                        player_id,
                        format!(
                            "Effet d'entree : {amount} degats a toute la Ligne Avant ennemie !"
                        ),
                    );
                }
                EntryDamageTarget::Single => {
                    // Gear 2: hit the strongest enemy front char, else any char, else the captain.
                    let enemies: Vec<String> = get_board_characters(state, opponent_id)
                        .into_iter()
                        .map(|c| c.instance_id.clone())
                        .collect();
                    let front: Vec<String> = enemies
                        .iter()
                        .filter(|id| {
                            state
                                .cards
                                .get(*id)
                                .and_then(|c| c.slot)
                                .is_some_and(|s| Slot::FRONT.contains(&s))
                        })
                        .cloned()
                        .collect();
                    let pool = if !front.is_empty() { front } else { enemies };

                    let mut target_id: Option<String> = None;
                    for id in &pool {
                        let take = match &target_id {
                            None => true,
                            Some(current) => {
                                get_effective_atk(state, registry, id)?
                                    > get_effective_atk(state, registry, current)?
                            }
                        };
                        if take {
                            target_id = Some(id.clone());
                        }
                    }

                    if let Some(tid) = target_id {
                        let def_id = state.get_card(&tid)?.def_id.clone();
                        let cursed = registry.get_card_def(&def_id)?.has_trait(Trait::Cursed);
                        let mut dmg = match cursed_bonus {
                            // TS `cursed && effect.cursedBonus ? effect.cursedBonus : effect.amount`
                            // — a `cursedBonus` of 0 is falsy and falls back to `amount`.
                            Some(bonus) if cursed && *bonus != 0 => *bonus,
                            _ => *amount,
                        };
                        if sand.unwrap_or(false) {
                            dmg += 1; // permanent PV loss approximated
                        }
                        state.get_card_mut(&tid)?.current_pv -= dmg;
                        let td_def_id = state.get_card(&tid)?.def_id.clone();
                        let td_name = registry.get_card_def(&td_def_id)?.name.clone();
                        state.add_log(
                            player_id,
                            format!("Effet d'entree : {dmg} degats a {td_name} !"),
                        );
                        if state.get_card(&tid)?.current_pv <= 0 {
                            let ko_def_id = state.get_card(&tid)?.def_id.clone();
                            state.add_log(opponent_id, format!("{td_name} est KO !"));
                            state.grant_ko_bonus(opponent_id);
                            remove_from_board(state, registry, &tid)?;
                            apply_on_ko_effects(
                                state,
                                registry,
                                ctx,
                                opponent_id,
                                player_id,
                                &ko_def_id,
                            )?;
                        }
                    } else {
                        state.players.get_mut(opponent_id).captain.current_pv -= *amount;
                        state.add_log(
                            player_id,
                            format!("Effet d'entree (Gear 2) : {amount} degats au Capitaine !"),
                        );
                    }
                }
            }
        }

        EntryEffect::GrantSelfRush => {
            // Clear the captain's summoning sickness so it can act the turn it flips (Gear 2).
            // TS `draft.players[playerId].captain.deployedTurn = -1` — the very
            // same sentinel, so the serialised state matches key for key.
            state.players.get_mut(player_id).captain.deployed_turn = Some(-1);
            state.add_log(
                player_id,
                "Effet d'entree : Gear 2 — le Capitaine peut agir immédiatement (Rush).",
            );
        }

        EntryEffect::Haoshoku {
            immobilize_max_def,
            debuff_atk,
        } => {
            // Shanks: immobilize enemies of DEF <= N and give all enemies -ATK this turn.
            let opponent_id = player_id.opponent();
            let ids: Vec<String> = state
                .players
                .get(opponent_id)
                .board
                .instance_ids()
                .into_iter()
                .cloned()
                .collect();
            for id in ids {
                let Some(card) = state.cards.get(&id) else {
                    continue;
                };
                let card_def_id = card.def_id.clone();
                let d = registry.get_card_def(&card_def_id)?;
                let ctrl_immune = d
                    .passive
                    .as_ref()
                    .is_some_and(|p| p.effects.contains(&PassiveEffect::ImmuneControl));
                let printed_def = d.def.unwrap_or(0);
                let now = ctx.now_ms;
                let Some(c) = state.cards.get_mut(&id) else {
                    continue;
                };
                c.modifiers.push(Modifier {
                    id: format!("haoshoku_{id}_{now}"),
                    stat: ModifierStat::Atk,
                    amount: -*debuff_atk,
                    source: "entry_haoshoku".to_string(),
                    duration: ModifierDuration::Turn,
                    turns_remaining: None,
                });
                if !ctrl_immune && printed_def <= *immobilize_max_def {
                    c.status_effects.push(StatusEffect {
                        effect_type: StatusEffectType::Immobilize,
                        turns_remaining: 2,
                        damage_per_turn: 0,
                        source: "haoshoku".to_string(),
                    });
                }
            }
            state.add_log(player_id, "Effet d'entree : Haoshoku Haki !");
        }

        EntryEffect::DebuffAllEnemies { atk } => {
            let opponent_id = player_id.opponent();
            let ids: Vec<String> = state
                .players
                .get(opponent_id)
                .board
                .instance_ids()
                .into_iter()
                .cloned()
                .collect();
            let now = ctx.now_ms;
            for id in ids {
                if let Some(c) = state.cards.get_mut(&id) {
                    c.modifiers.push(Modifier {
                        id: format!("entrydebuff_{id}_{now}"),
                        stat: ModifierStat::Atk,
                        amount: -*atk,
                        source: "entry".to_string(),
                        duration: ModifierDuration::Turn,
                        turns_remaining: None,
                    });
                }
            }
        }

        EntryEffect::DiscardOpponentRandom { amount } => {
            let opponent_id = player_id.opponent();
            let mut i = 0;
            while i < *amount && !state.players.get(opponent_id).hand.is_empty() {
                let len = state.players.get(opponent_id).hand.len();
                let idx = ctx.rng.random_index(len);
                let d2 = state.players.get_mut(opponent_id).hand.remove(idx);
                if let Some(card) = state.cards.get_mut(&d2) {
                    card.zone = Zone::Graveyard;
                }
                state.players.get_mut(opponent_id).graveyard.push(d2);
                i += 1;
            }
        }

        EntryEffect::Multi { effects } => {
            for sub in effects {
                resolve_entry_effect(state, registry, ctx, player_id, sub)?;
            }
        }

        EntryEffect::Custom { id: _, description } => {
            state.add_log(player_id, format!("Effet d'entree special : {description}"));
        }
    }

    Ok(())
}

/// TS `declareCaptainBaseAttack(state, playerId, targetInstanceId, targetIsCaptain)`
/// — `src/engine/captain.ts:277`.
///
/// Verso-only. Taps the captain, sets both action flags, and parks a
/// [`crate::state::PendingAttack`] whose `attackerId` is the synthetic
/// `` `captain_{playerId}` `` ([`crate::combat::captain_attacker_id`]) with
/// `rawDamage = max(0, versoAtk + atkModifiers - targetDef)`,
/// `hasHaki = verso.naturalHaki non-empty || turnNumber >= 7`. Logs
/// `"Capitaine {name} attaque avec {baseAction.name} ! ({raw} degats)"`.
///
/// Errors: `Captain not flipped (verso required)`, `Captain is tapped`,
/// `Captain base action already used`, `Captain has summoning sickness`.
pub fn declare_captain_base_attack(
    state: &mut GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<(), EngineError> {
    let captain = &state.players.get(player_id).captain;
    if !captain.flipped {
        return Err(EngineError::illegal("Captain not flipped (verso required)"));
    }
    if captain.tapped {
        return Err(EngineError::illegal("Captain is tapped"));
    }
    if captain.used_base_action {
        return Err(EngineError::illegal("Captain base action already used"));
    }

    let def = registry.get_captain_def(&captain.def_id)?;
    let base_action = def.verso.base_action.clone();

    // Captain summoning sickness
    if captain.deployed_turn == Some(i64::from(state.turn_number)) {
        // Verso captain just flipped — has mal de terre unless Rush
        let has_rush = def
            .verso
            .traits
            .as_ref()
            .is_some_and(|ts| ts.contains(&Trait::Rush));
        if !has_rush {
            return Err(EngineError::illegal("Captain has summoning sickness"));
        }
    }

    // Calculate ATK
    let mut atk = def.verso.atk;
    for m in &captain.modifiers {
        if m.stat == ModifierStat::Atk {
            atk += m.amount;
        }
    }

    let cap_name = def.name.clone();
    let has_haki = def
        .verso
        .natural_haki
        .as_ref()
        .is_some_and(|h| !h.is_empty())
        || state.turn_number >= 7;

    // Get target DEF
    let target_def_val = if target_is_captain {
        let opponent_id = player_id.opponent();
        let opp_cap = &state.players.get(opponent_id).captain;
        let opp_cap_def = registry.get_captain_def(&opp_cap.def_id)?;
        let mut v = if opp_cap.flipped {
            opp_cap_def.verso.def
        } else {
            opp_cap_def.recto.def
        };
        for m in &opp_cap.modifiers {
            if m.stat == ModifierStat::Def {
                v += m.amount;
            }
        }
        v
    } else {
        get_effective_def(state, registry, target_instance_id)?
    };

    let raw_damage = (atk - target_def_val).max(0);

    {
        let cap = &mut state.players.get_mut(player_id).captain;
        cap.tapped = true;
        // One action per turn (Rulebook v3.1 §2.2/§6).
        cap.used_base_action = true;
        cap.used_special_attack = true;
    }
    state.pending_attack = Some(PendingAttack {
        attacker_id: crate::combat::captain_attacker_id(player_id),
        target_id: target_instance_id.to_string(),
        target_is_captain,
        is_special: false,
        raw_damage,
        attack_power: Some(atk),
        element: base_action.element,
        attack_traits: base_action.attack_traits.clone().unwrap_or_default(),
        has_haki,
        ignore_shield: None,
        cannot_be_dodged: None,
        immobilize: None,
        sleep: None,
        pushback: None,
        pushback_slots: None,
        strip_stealth: None,
        survive_played: None,
    });

    state.add_log(
        player_id,
        format!(
            "Capitaine {cap_name} attaque avec {} ! ({raw_damage} degats)",
            base_action.name
        ),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Board, CaptainInstance, CardInstance, PlayerState, Players};
    use crate::types::{
        BaseAction, CaptainDef, CaptainRecto, CaptainVerso, CardDef, CardType, Faction,
        FlipCondition, PassiveDef, Phase, Rarity, SpecialAttack, TurnDuration,
    };
    use std::collections::BTreeMap;

    const P1: PlayerId = PlayerId::Player1;
    const P2: PlayerId = PlayerId::Player2;

    fn passive() -> PassiveDef {
        PassiveDef {
            name: "p".into(),
            description: "d".into(),
            effects: vec![],
        }
    }

    fn captain_def(
        id: &str,
        flip_condition: FlipCondition,
        entry_effect: EntryEffect,
    ) -> CaptainDef {
        CaptainDef {
            id: id.into(),
            name: format!("Cap {id}"),
            faction: Faction::Pirate,
            tags: None,
            traits: None,
            recto: CaptainRecto {
                pv: 20,
                atk: 3,
                def: 2,
                passive: passive(),
                attacks: vec![],
                surcharge: None,
            },
            flip_condition,
            verso: CaptainVerso {
                pv: 25,
                atk: 7,
                def: 4,
                passive: passive(),
                entry_effect,
                base_action: BaseAction {
                    name: "Gomu Gomu".into(),
                    atk: 7,
                    ..Default::default()
                },
                special_attack: SpecialAttack {
                    name: "s".into(),
                    cost: 2,
                    atk_bonus: 2,
                    ..Default::default()
                },
                surcharge: None,
                traits: None,
                natural_haki: None,
            },
        }
    }

    fn character(id: &str, atk: i32, def: i32, pv: i32) -> CardDef {
        let mut d = CardDef::new(
            id,
            format!("Char {id}"),
            CardType::Character,
            1,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        d.atk = Some(atk);
        d.def = Some(def);
        d.pv = Some(pv);
        d
    }

    fn player_state(id: PlayerId, cap_id: &str) -> PlayerState {
        PlayerState {
            id,
            captain: CaptainInstance::new(cap_id.into(), id, 20),
            deck: Vec::new(),
            hand: Vec::new(),
            graveyard: Vec::new(),
            board: Board::empty(),
            active_ship: None,
            volonte: 10,
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

    fn blank_state(cap1: &str, cap2: &str) -> GameState {
        GameState {
            cards: BTreeMap::new(),
            players: Players {
                player1: player_state(P1, cap1),
                player2: player_state(P2, cap2),
            },
            turn_number: 5,
            current_player: P1,
            phase: Phase::Main,
            pending_attack: None,
            log: Vec::new(),
            winner: None,
            first_player: P1,
        }
    }

    fn put(
        state: &mut GameState,
        registry: &CardRegistry,
        def_id: &str,
        owner: PlayerId,
        slot: Slot,
    ) -> String {
        let n = state.cards.len();
        let iid = format!("{def_id}#{n}");
        let pv = registry.card_def(def_id).and_then(|d| d.pv).unwrap_or(0);
        let mut inst = CardInstance::new(iid.clone(), def_id.to_string(), owner, pv);
        inst.zone = Zone::Board;
        inst.slot = Some(slot);
        state.cards.insert(iid.clone(), inst);
        state
            .players
            .get_mut(owner)
            .board
            .set(slot, Some(iid.clone()));
        iid
    }

    fn last_log(state: &GameState) -> &str {
        state.log.last().map(|l| l.message.as_str()).unwrap_or("")
    }

    // --- canFlipCaptain ---

    #[test]
    fn can_flip_is_false_once_flipped() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let reg = CardRegistry::from_sets([vec![]], [cap]);
        let mut state = blank_state("C1", "C1");
        state.players.get_mut(P1).captain.flipped = true;
        assert!(!can_flip_captain(&state, &reg, P1).unwrap());
    }

    #[test]
    fn can_flip_free_when_ally_koed_this_turn() {
        let cap = captain_def(
            "C1",
            FlipCondition {
                cost: Some(9),
                free_if_ally_ko: Some(true),
                ..Default::default()
            },
            EntryEffect::GrantSelfRush,
        );
        let reg = CardRegistry::from_sets([vec![]], [cap]);
        let mut state = blank_state("C1", "C1");
        state.players.get_mut(P1).volonte = 0;
        assert!(!can_flip_captain(&state, &reg, P1).unwrap());
        state.players.get_mut(P1).ally_ko_ed_this_turn = Some(true);
        assert!(can_flip_captain(&state, &reg, P1).unwrap());
    }

    #[test]
    fn auto_flip_needs_turn_four_and_falls_through_to_cost() {
        let cap = captain_def(
            "C1",
            FlipCondition {
                cost: Some(4),
                auto_if_allies_lte: Some(1),
                ..Default::default()
            },
            EntryEffect::GrantSelfRush,
        );
        let reg = CardRegistry::from_sets([vec![]], [cap]);
        let mut state = blank_state("C1", "C1");
        state.turn_number = 3;
        state.players.get_mut(P1).volonte = 0;
        // turn < 4 → the auto branch is skipped, cost branch decides (0 Vol → false)
        assert!(!can_flip_captain(&state, &reg, P1).unwrap());
        state.turn_number = 4;
        assert!(can_flip_captain(&state, &reg, P1).unwrap());
        // Enough allies to break the auto condition → cost decides again.
        let mut reg2 = reg.clone();
        reg2.register_set(vec![character("X", 1, 1, 3)]);
        put(&mut state, &reg2, "X", P1, Slot::V1);
        put(&mut state, &reg2, "X", P1, Slot::V2);
        assert!(!can_flip_captain(&state, &reg2, P1).unwrap());
        state.players.get_mut(P1).volonte = 4;
        assert!(can_flip_captain(&state, &reg2, P1).unwrap());
    }

    // --- flipCaptain ---

    #[test]
    fn flip_carries_marked_damage_and_logs_the_slot() {
        let cap = captain_def(
            "C1",
            FlipCondition {
                cost: Some(3),
                ..Default::default()
            },
            EntryEffect::GrantSelfRush,
        );
        let reg = CardRegistry::from_sets([vec![]], [cap]);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        // 6 damage marked on the recto face (20 → 14)
        state.players.get_mut(P1).captain.current_pv = 14;
        flip_captain(&mut state, &reg, &mut ctx, P1, Slot::A2).unwrap();
        let cap_inst = &state.players.get(P1).captain;
        assert!(cap_inst.flipped);
        assert_eq!(cap_inst.current_pv, 25 - 6);
        assert_eq!(cap_inst.slot, Some(Slot::A2));
        // grantSelfRush cleared the deployedTurn again (TS sentinel `-1`)
        assert_eq!(cap_inst.deployed_turn, Some(-1));
        assert_eq!(state.players.get(P1).volonte, 7);
        assert_eq!(
            state.log[0].message,
            "Cap C1 s'engage sur le champ de bataille ! (verso, slot A2)"
        );
    }

    #[test]
    fn flip_into_a_lethal_face_skips_the_entry_effect() {
        // recto 20 PV / verso 25 PV, but only 1 PV left → 25 - 19 = 6. Make it lethal
        // by using a verso smaller than the marked damage.
        let mut cap = captain_def(
            "C1",
            FlipCondition {
                cost: Some(0),
                ..Default::default()
            },
            EntryEffect::Draw { amount: 1 },
        );
        cap.verso.pv = 5;
        let reg = CardRegistry::from_sets([vec![]], [cap]);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        state.players.get_mut(P1).captain.current_pv = 4; // 16 marked
        flip_captain(&mut state, &reg, &mut ctx, P1, Slot::V1).unwrap();
        assert_eq!(state.players.get(P1).captain.current_pv, 5 - 16);
        assert_eq!(state.winner, Some(P2));
        // The entry effect (draw) never ran: only the engage line is logged.
        assert_eq!(state.log.len(), 1);
    }

    #[test]
    fn flip_rejects_occupied_slot_and_unaffordable_cost() {
        let cap = captain_def(
            "C1",
            FlipCondition {
                cost: Some(9),
                ..Default::default()
            },
            EntryEffect::GrantSelfRush,
        );
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 1, 3)]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        put(&mut state, &reg, "X", P1, Slot::V1);
        assert_eq!(
            flip_captain(&mut state, &reg, &mut ctx, P1, Slot::V1),
            Err(EngineError::SlotOccupied(Slot::V1))
        );
        state.players.get_mut(P1).volonte = 2;
        assert_eq!(
            flip_captain(&mut state, &reg, &mut ctx, P1, Slot::V2),
            Err(EngineError::illegal("Cannot afford captain flip"))
        );
        state.players.get_mut(P1).captain.flipped = true;
        assert_eq!(
            flip_captain(&mut state, &reg, &mut ctx, P1, Slot::V2),
            Err(EngineError::illegal("Captain already flipped"))
        );
    }

    // --- resolveEntryEffect ---

    #[test]
    fn buff_allies_touches_every_own_board_slot() {
        let cap = captain_def(
            "C1",
            FlipCondition::default(),
            EntryEffect::BuffAllies {
                stat: AtkDefStat::Atk,
                amount: 2,
                duration: TurnDuration::Turn,
            },
        );
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 1, 3)]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        let a = put(&mut state, &reg, "X", P1, Slot::V1);
        let b = put(&mut state, &reg, "X", P1, Slot::A3);
        let enemy = put(&mut state, &reg, "X", P2, Slot::V1);
        let effect = reg.captain_def("C1").unwrap().verso.entry_effect.clone();
        resolve_entry_effect(&mut state, &reg, &mut ctx, P1, &effect).unwrap();
        assert_eq!(state.cards[&a].modifiers.len(), 1);
        assert_eq!(state.cards[&a].modifiers[0].source, "C1");
        assert_eq!(state.cards[&a].modifiers[0].amount, 2);
        assert_eq!(state.cards[&b].modifiers.len(), 1);
        assert!(state.cards[&enemy].modifiers.is_empty());
        assert_eq!(
            last_log(&state),
            "Effet d'entree : tous allies +2 ATK ce tour !"
        );
    }

    #[test]
    fn damage_all_front_hits_only_v_slots_and_never_sweeps_kos() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 1, 3)]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        let v1 = put(&mut state, &reg, "X", P2, Slot::V1);
        let a1 = put(&mut state, &reg, "X", P2, Slot::A1);
        let effect = EntryEffect::DamageEnemies {
            amount: 5,
            target: EntryDamageTarget::AllFront,
            cursed_bonus: None,
            sand: None,
        };
        resolve_entry_effect(&mut state, &reg, &mut ctx, P1, &effect).unwrap();
        assert_eq!(state.cards[&v1].current_pv, -2);
        assert_eq!(state.cards[&a1].current_pv, 3);
        // No KO sweep: the dead front character is still on the board.
        assert_eq!(state.players.get(P2).board.get(Slot::V1), Some(&v1));
        assert_eq!(
            last_log(&state),
            "Effet d'entree : 5 degats a toute la Ligne Avant ennemie !"
        );
    }

    #[test]
    fn damage_single_picks_the_strongest_front_enemy_and_kos_it() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let mut reg = CardRegistry::from_sets(
            [vec![
                character("WEAK", 1, 0, 10),
                character("STRONG", 6, 0, 3),
                character("HUGE", 9, 0, 10),
            ]],
            [],
        );
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        let weak = put(&mut state, &reg, "WEAK", P2, Slot::V1);
        let strong = put(&mut state, &reg, "STRONG", P2, Slot::V2);
        // A back-row monster must be ignored while the front pool is non-empty.
        let huge = put(&mut state, &reg, "HUGE", P2, Slot::A1);
        let effect = EntryEffect::DamageEnemies {
            amount: 4,
            target: EntryDamageTarget::Single,
            cursed_bonus: None,
            sand: None,
        };
        resolve_entry_effect(&mut state, &reg, &mut ctx, P1, &effect).unwrap();
        assert_eq!(state.cards[&weak].current_pv, 10);
        assert_eq!(state.cards[&huge].current_pv, 10);
        assert_eq!(state.cards[&strong].current_pv, -1);
        // Full KO chain ran: log, +2 Vol for the owner, removal from the board.
        assert!(state.players.get(P2).board.get(Slot::V2).is_none());
        assert_eq!(state.players.get(P2).volonte, 12);
        assert!(state.players.get(P2).ally_koed_this_turn());
        let msgs: Vec<&str> = state.log.iter().map(|l| l.message.as_str()).collect();
        assert!(msgs.contains(&"Effet d'entree : 4 degats a Char STRONG !"));
        assert!(msgs.contains(&"Char STRONG est KO !"));
    }

    #[test]
    fn damage_single_cursed_bonus_replaces_amount_and_sand_adds_one() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let mut cursed = character("CU", 1, 0, 20);
        cursed.traits = Some(vec![Trait::Cursed]);
        let mut reg = CardRegistry::from_sets([vec![cursed]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        let c = put(&mut state, &reg, "CU", P2, Slot::V1);
        let effect = EntryEffect::DamageEnemies {
            amount: 9,
            target: EntryDamageTarget::Single,
            cursed_bonus: Some(3), // replaces, does not add
            sand: Some(true),      // +1
        };
        resolve_entry_effect(&mut state, &reg, &mut ctx, P1, &effect).unwrap();
        assert_eq!(state.cards[&c].current_pv, 20 - 4);
        assert_eq!(last_log(&state), "Effet d'entree : 4 degats a Char CU !");
    }

    #[test]
    fn damage_single_with_no_enemy_character_hits_the_captain() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let mut reg = CardRegistry::from_sets([vec![]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        let effect = EntryEffect::DamageEnemies {
            amount: 3,
            target: EntryDamageTarget::Single,
            cursed_bonus: Some(8),
            sand: Some(true), // ignored on the captain branch
        };
        resolve_entry_effect(&mut state, &reg, &mut ctx, P1, &effect).unwrap();
        assert_eq!(state.players.get(P2).captain.current_pv, 17);
        assert_eq!(
            last_log(&state),
            "Effet d'entree (Gear 2) : 3 degats au Capitaine !"
        );
    }

    #[test]
    fn haoshoku_debuffs_everyone_but_only_immobilizes_low_def_non_immune() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let low = character("LOW", 1, 2, 5);
        let high = character("HIGH", 1, 6, 5);
        let mut immune = character("IMM", 1, 1, 5);
        immune.passive = Some(PassiveDef {
            name: "n".into(),
            description: "d".into(),
            effects: vec![PassiveEffect::ImmuneControl],
        });
        let mut reg = CardRegistry::from_sets([vec![low, high, immune]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        ctx.set_now_ms(42);
        let l = put(&mut state, &reg, "LOW", P2, Slot::V1);
        let h = put(&mut state, &reg, "HIGH", P2, Slot::V2);
        let i = put(&mut state, &reg, "IMM", P2, Slot::V3);
        let effect = EntryEffect::Haoshoku {
            immobilize_max_def: 3,
            debuff_atk: 2,
        };
        resolve_entry_effect(&mut state, &reg, &mut ctx, P1, &effect).unwrap();
        for id in [&l, &h, &i] {
            assert_eq!(state.cards[id].modifiers.len(), 1);
            assert_eq!(state.cards[id].modifiers[0].amount, -2);
            assert_eq!(state.cards[id].modifiers[0].source, "entry_haoshoku");
        }
        assert_eq!(state.cards[&l].modifiers[0].id, format!("haoshoku_{l}_42"));
        assert!(state.cards[&l].has_status(StatusEffectType::Immobilize));
        assert!(!state.cards[&h].has_status(StatusEffectType::Immobilize));
        assert!(!state.cards[&i].has_status(StatusEffectType::Immobilize));
        assert_eq!(last_log(&state), "Effet d'entree : Haoshoku Haki !");
    }

    #[test]
    fn debuff_all_enemies_writes_no_log() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 1, 3)]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        ctx.set_now_ms(7);
        let e = put(&mut state, &reg, "X", P2, Slot::A1);
        resolve_entry_effect(
            &mut state,
            &reg,
            &mut ctx,
            P1,
            &EntryEffect::DebuffAllEnemies { atk: 3 },
        )
        .unwrap();
        assert_eq!(
            state.cards[&e].modifiers[0].id,
            format!("entrydebuff_{e}_7")
        );
        assert_eq!(state.cards[&e].modifiers[0].amount, -3);
        assert_eq!(state.cards[&e].modifiers[0].source, "entry");
        assert!(state.log.is_empty());
    }

    #[test]
    fn discard_opponent_random_stops_at_an_empty_hand() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 1, 3)]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(4);
        for n in 0..2 {
            let iid = format!("X#h{n}");
            let mut inst = CardInstance::new(iid.clone(), "X".into(), P2, 3);
            inst.zone = Zone::Hand;
            state.cards.insert(iid.clone(), inst);
            state.players.get_mut(P2).hand.push(iid);
        }
        resolve_entry_effect(
            &mut state,
            &reg,
            &mut ctx,
            P1,
            &EntryEffect::DiscardOpponentRandom { amount: 5 },
        )
        .unwrap();
        assert!(state.players.get(P2).hand.is_empty());
        assert_eq!(state.players.get(P2).graveyard.len(), 2);
        for id in &state.players.get(P2).graveyard {
            assert_eq!(state.cards[id].zone, Zone::Graveyard);
        }
        assert!(state.log.is_empty());
    }

    #[test]
    fn multi_and_custom_recurse_in_order() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let reg = CardRegistry::from_sets([vec![]], [cap]);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        let effect = EntryEffect::Multi {
            effects: vec![
                EntryEffect::Custom {
                    id: "a".into(),
                    description: "Alpha".into(),
                },
                EntryEffect::Custom {
                    id: "b".into(),
                    description: "Beta".into(),
                },
            ],
        };
        resolve_entry_effect(&mut state, &reg, &mut ctx, P1, &effect).unwrap();
        let msgs: Vec<&str> = state.log.iter().map(|l| l.message.as_str()).collect();
        assert_eq!(
            msgs,
            vec![
                "Effet d'entree special : Alpha",
                "Effet d'entree special : Beta"
            ]
        );
    }

    // --- declareCaptainBaseAttack ---

    #[test]
    fn captain_base_attack_requires_verso_untapped_and_unused() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 2, 5)]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let t = put(&mut state, &reg, "X", P2, Slot::V1);
        assert_eq!(
            declare_captain_base_attack(&mut state, &reg, P1, &t, false),
            Err(EngineError::illegal("Captain not flipped (verso required)"))
        );
        state.players.get_mut(P1).captain.flipped = true;
        state.players.get_mut(P1).captain.tapped = true;
        assert_eq!(
            declare_captain_base_attack(&mut state, &reg, P1, &t, false),
            Err(EngineError::illegal("Captain is tapped"))
        );
        state.players.get_mut(P1).captain.tapped = false;
        state.players.get_mut(P1).captain.used_base_action = true;
        assert_eq!(
            declare_captain_base_attack(&mut state, &reg, P1, &t, false),
            Err(EngineError::illegal("Captain base action already used"))
        );
        state.players.get_mut(P1).captain.used_base_action = false;
        state.players.get_mut(P1).captain.deployed_turn = Some(i64::from(state.turn_number));
        assert_eq!(
            declare_captain_base_attack(&mut state, &reg, P1, &t, false),
            Err(EngineError::illegal("Captain has summoning sickness"))
        );
    }

    #[test]
    fn captain_base_attack_builds_the_pending_attack_and_taps() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 2, 5)]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let t = put(&mut state, &reg, "X", P2, Slot::V1);
        let capt = &mut state.players.get_mut(P1).captain;
        capt.flipped = true;
        capt.modifiers.push(Modifier {
            id: "m".into(),
            stat: ModifierStat::Atk,
            amount: 1,
            source: "s".into(),
            duration: ModifierDuration::Turn,
            turns_remaining: None,
        });
        declare_captain_base_attack(&mut state, &reg, P1, &t, false).unwrap();
        let pa = state.pending_attack.clone().unwrap();
        assert_eq!(pa.attacker_id, "captain_player1");
        assert_eq!(pa.target_id, t);
        assert!(!pa.is_special);
        assert_eq!(pa.attack_power, Some(8)); // verso 7 + modifier 1
        assert_eq!(pa.raw_damage, 6); // 8 - DEF 2
        assert!(!pa.has_haki); // turn 5 < 7, no natural haki
        let cap_inst = &state.players.get(P1).captain;
        assert!(cap_inst.tapped);
        assert!(cap_inst.used_base_action);
        assert!(cap_inst.used_special_attack);
        assert_eq!(
            last_log(&state),
            "Capitaine Cap C1 attaque avec Gomu Gomu ! (6 degats)"
        );
    }

    #[test]
    fn captain_base_attack_on_a_captain_reads_the_right_face_and_haki_turn() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let reg = CardRegistry::from_sets([vec![]], [cap]);
        let mut state = blank_state("C1", "C1");
        state.turn_number = 7;
        state.players.get_mut(P1).captain.flipped = true;
        // Recto DEF 2 → raw = 7 - 2 = 5
        declare_captain_base_attack(&mut state, &reg, P1, "captain_player2", true).unwrap();
        let pa = state.pending_attack.clone().unwrap();
        assert_eq!(pa.raw_damage, 5);
        assert!(pa.target_is_captain);
        assert!(pa.has_haki); // turn 7

        // Flipped opponent → verso DEF 4, plus a -1 DEF modifier → 3 → raw = 4
        state.pending_attack = None;
        let c1 = &mut state.players.get_mut(P1).captain;
        c1.tapped = false;
        c1.used_base_action = false;
        let c2 = &mut state.players.get_mut(P2).captain;
        c2.flipped = true;
        c2.modifiers.push(Modifier {
            id: "m".into(),
            stat: ModifierStat::Def,
            amount: -1,
            source: "s".into(),
            duration: ModifierDuration::Turn,
            turns_remaining: None,
        });
        declare_captain_base_attack(&mut state, &reg, P1, "captain_player2", true).unwrap();
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 4);
    }
}

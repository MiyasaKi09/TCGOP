//! Captains — port of `src/engine/captain.ts`.
//!
//! A captain starts *recto*: off-board, unable to attack, contributing only its
//! passive. Flipping it (irreversibly) puts the *verso* face into a board slot,
//! carries the marked damage over (`versoPv - (rectoPv - currentPv)`), triggers
//! the verso `entryEffect` and unlocks the captain's base attack.

use crate::board::{
    get_board_characters, get_effective_atk, get_effective_def, has_trait, is_slot_free,
    remove_from_board,
};
use crate::combat::conditional_atk_bonus;
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::passives::apply_on_ko_effects;
use crate::registry::CardRegistry;
use crate::state::{GameState, PendingAttack, once_surcharge};
use crate::types::{
    AtkDefStat, AttackTrait, Element, EntryDamageTarget, EntryEffect, Modifier, ModifierDuration,
    ModifierStat, PassiveEffect, PlayerId, Slot, SpecialAttack, StatusEffect, StatusEffectType,
    Trait, Zone,
};

/// Why a captain flip costs nothing — the union of the five `flipCondition`
/// clauses (§8.2 item 33).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreeFlipReason {
    /// `freeIfAllyKO` — an ally was KO'd this turn (Luffy).
    AllyKo,
    /// `autoIfAlliesLte` — turn >= 4 and few enough allies left.
    AutoIfAlliesLte,
    /// `freeIfEnemyCursed` — a Cursed enemy is in play (Akainu).
    EnemyCursed,
    /// `freeIfAlliesGte` — you control enough characters (Crocodile).
    AlliesGte,
    /// `freeIfTurnGte` — the turn counter reached the printed number (Shanks).
    TurnGte,
}

/// A captain frozen, immobilised or asleep cannot act (§8.1 item 35).
///
/// The same predicate gates [`get_valid_actions`](crate::actions::get_valid_actions)
/// and the three declare functions, so the enumerator never offers an attack
/// the executor refuses.
pub fn captain_cannot_act(effects: &[StatusEffect]) -> bool {
    effects.iter().any(|e| {
        matches!(
            e.effect_type,
            StatusEffectType::Freeze | StatusEffectType::Immobilize | StatusEffectType::Sleep
        )
    })
}

/// Is an enemy Cursed unit in play? (`freeIfEnemyCursed`, item 33.)
///
/// Any enemy board character carrying [`Trait::Cursed`] (equipment included,
/// via [`has_trait`]) or an enemy captain whose **active** traits are Cursed —
/// [`captain_traits`], i.e. the card-level list on both faces plus the verso's
/// own once flipped (decision §8.40).
fn enemy_cursed_in_play(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Result<bool, EngineError> {
    let opponent_id = player_id.opponent();
    let ids: Vec<String> = get_board_characters(state, opponent_id)
        .into_iter()
        .map(|c| c.instance_id.clone())
        .collect();
    for id in ids {
        if has_trait(state, registry, &id, Trait::Cursed)? {
            return Ok(true);
        }
    }

    // Decision §8.40: one definition of "the captain's active traits" —
    // `captain_traits(def, flipped) = def.traits union (flipped ? verso : [])`.
    // The exclusive if/else this used to carry made a card-level `cursed`
    // invisible the moment the captain flipped, so `freeIfEnemyCursed` and the
    // Water element disagreed about the same face. Decision §8.28 (follow-up)
    // adds the equipment: a captain wearing one of the three signature fruits
    // is `cursed` by it, exactly as a character wearing it is.
    captain_has_trait_now(state, registry, opponent_id, Trait::Cursed)
}

/// The single free-flip predicate shared by [`can_flip_captain`] and
/// [`flip_captain`] (§8.2 item 33) — the five `flipCondition` clauses tested in
/// declaration order.
pub fn free_flip_reason(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Result<Option<FreeFlipReason>, EngineError> {
    let captain = &state.players.get(player_id).captain;
    let condition = &registry.get_captain_def(&captain.def_id)?.flip_condition;
    let ally_count = get_board_characters(state, player_id).len() as i32;

    // Free flip if a Mugiwara ally was KO'd this turn (Luffy).
    if condition.free_if_ally_ko.unwrap_or(false)
        && state.players.get(player_id).ally_koed_this_turn()
    {
        return Ok(Some(FreeFlipReason::AllyKo));
    }

    // Auto-flip condition (allies <= N) — only from turn 4+ to prevent early abuse.
    if let Some(max_allies) = condition.auto_if_allies_lte
        && state.turn_number >= 4
        && ally_count <= max_allies
    {
        return Ok(Some(FreeFlipReason::AutoIfAlliesLte));
    }

    // Akainu: free while a Cursed enemy is in play.
    if condition.free_if_enemy_cursed.unwrap_or(false)
        && enemy_cursed_in_play(state, registry, player_id)?
    {
        return Ok(Some(FreeFlipReason::EnemyCursed));
    }

    // Crocodile: free once the Baroque Works board is wide enough.
    if let Some(min_allies) = condition.free_if_allies_gte
        && ally_count >= min_allies
    {
        return Ok(Some(FreeFlipReason::AlliesGte));
    }

    // Shanks: free from the printed turn onwards.
    if let Some(min_turn) = condition.free_if_turn_gte
        && state.turn_number >= min_turn
    {
        return Ok(Some(FreeFlipReason::TurnGte));
    }

    Ok(None)
}

/// TS `canFlipCaptain(state, playerId)` — `src/engine/captain.ts:11`.
///
/// `false` once flipped. Otherwise a free flip ([`free_flip_reason`]) is always
/// legal and everything else falls through to `canAfford(condition.cost ?? 0)`
/// — the executor's reading (§8.1 item 6): a `flipCondition` with no `cost` is
/// a cost of 0, which the old predicate reported as *illegal* while
/// [`flip_captain`] happily performed it.
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

    if free_flip_reason(state, registry, player_id)?.is_some() {
        return Ok(true);
    }

    // Check Vol. cost — `cost ?? 0`, exactly like the executor.
    let cost = registry
        .get_captain_def(&captain.def_id)?
        .flip_condition
        .cost
        .unwrap_or(0);
    Ok(state.can_afford(player_id, cost))
}

/// TS `flipCaptain(state, playerId, slot)` — `src/engine/captain.ts:44`.
///
/// Recomputes the free-flip test ([`free_flip_reason`]), pays
/// `condition.cost ?? 0` otherwise, sets `flipped`,
/// `currentPv = verso.pv - max(0, recto.pv - currentPv)`, `slot` and
/// `deployedTurn`, logs
/// `"{name} s'engage sur le champ de bataille ! (verso, slot {slot})"`, runs
/// [`resolve_entry_effect`] and only **then** checks the win condition.
///
/// §8.1 item 32: the captain does arrive on the board, so its entry effect
/// happens even when the smaller verso face is lethal; the game is decided
/// afterwards (and the spent Volonté stays spent).
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

    // Check slot availability (decision §8.31: one occupancy predicate for the
    // board cells and for a captain already standing in a slot).
    if !is_slot_free(state, player_id, slot) {
        return Err(EngineError::SlotOccupied(slot));
    }

    // Determine cost — one shared predicate with `can_flip_captain` (item 33).
    let mut cost = 0;
    let free_flip = free_flip_reason(state, registry, player_id)?.is_some();

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
    // Decision §8.28 (follow-up) × §8.29: the equipment stands where its
    // bearer stands, so a captain equipped while still recto (off-board, so
    // `slot == None`) brings its objects into the slot it flips into.
    let attached = state
        .players
        .get(player_id)
        .captain
        .attached_objects
        .clone();
    crate::board::move_attached_objects(state, &attached, slot);

    state.add_log(
        player_id,
        format!(
            "{} s'engage sur le champ de bataille ! (verso, slot {})",
            def.name,
            slot.as_str()
        ),
    );

    // Apply entry effect — the captain is on the board, so it triggers even
    // when flipping into a lower-PV face was lethal (item 32).
    resolve_entry_effect(state, registry, ctx, player_id, &def.verso.entry_effect)?;

    // The game is decided after the entry effect: a simultaneous double-KO then
    // resolves through the standard "active player loses" rule.
    if let Some(flip_winner) = state.check_win_condition() {
        state.winner = Some(flip_winner);
    }

    Ok(())
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
                        let dmg = match cursed_bonus {
                            // Decision §8.14: `cursedBonus` stays a *replacement*
                            // total ("N degats, M contre Maudit" — Akainu's entry
                            // reads 4 -> 6, not 4 + 6), but the JS-falsy-zero
                            // quirk is gone: a `Some(0)` really means 0 damage
                            // against a Cursed target.
                            Some(bonus) if cursed => *bonus,
                            _ => *amount,
                        };
                        state.get_card_mut(&tid)?.current_pv -= dmg;
                        // Decision §8.36/§8.40 — Crocodile's entry effect is
                        // `{ damageEnemies, amount: 4, single, sand: true }`;
                        // `sand` is "-1 PV **permanent**", the same rule the
                        // Sand element and `EventEffect::DamageEnemies` apply,
                        // not one extra point of ordinary damage the next heal
                        // gives back. The log keeps reporting the damage only,
                        // so the printed 4 stays 4.
                        if sand.unwrap_or(false) {
                            crate::board::apply_permanent_pv_loss(state, registry, &tid, 1)?;
                        }
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
                        // Decision §8.40 gave the captain a max-PV model, so
                        // `sand` is the same permanent loss on this fallback
                        // as it is on the body branch above.
                        if sand.unwrap_or(false) {
                            crate::board::apply_captain_permanent_pv_loss(
                                state,
                                registry,
                                opponent_id,
                                1,
                            )?;
                        }
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
/// `rawDamage = max(0, baseAction.atk + atkModifiers - targetDef)`,
/// `hasHaki = verso.naturalHaki non-empty || turnNumber >= 7`. Logs
/// `"Capitaine {name} attaque avec {baseAction.name} ! ({raw} degats)"`.
///
/// §8.1 item 35 — the named attack's own data is now read: `baseAction.atk` is
/// the attack's power (all four shipped captains print `baseAction.atk ==
/// verso.atk`, so their numbers do not move), `piercing` (on the attack or on
/// the active face) halves the target's DEF, and `element`, `attackTraits`,
/// `cannotBeDodged`, `immobilize`, `stripStealth` plus an `impact` pushback
/// ride into the pending attack. A frozen / immobilised / sleeping captain is
/// refused ([`captain_cannot_act`]).
///
/// `baseAction` has no `ignoreDef` field in the data model (neither in TS nor
/// here), so that half of the decision is inert by construction.
///
/// Errors: `Captain not flipped (verso required)`, `Captain is tapped`,
/// `Captain base action already used`, `Captain cannot act (frozen,
/// immobilized or asleep)`, `Captain has summoning sickness`.
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
    if captain_cannot_act(&captain.status_effects) {
        return Err(EngineError::illegal(CAPTAIN_CANNOT_ACT));
    }

    let def = registry.get_captain_def(&captain.def_id)?;
    let base_action = def.verso.base_action.clone();

    // Captain summoning sickness
    if captain.deployed_turn == Some(i64::from(state.turn_number)) {
        // Verso captain just flipped — has mal de terre unless Rush.
        // Decision §8.40: every captain trait read goes through the union
        // helper, `rush` included — a card-level `traits: ["rush"]` belongs to
        // the captain on both faces, so reading `verso.traits` alone would let
        // the special attack (which already uses the union) fire on the flip
        // turn while the base attack refused. Inert on the shipped catalogue:
        // no captain prints a card-level `rush`.
        if !captain_has_trait_now(state, registry, player_id, Trait::Rush)? {
            return Err(EngineError::illegal("Captain has summoning sickness"));
        }
    }

    // Decision §8.38: the taunt binds every attacker, the captain included.
    // No shipped effect can taunt a captain (`resolve_support_special`, the
    // only writer of the status, targets a card instance), so this is the
    // rule stated once and for all rather than a live clause.
    crate::combat::enforce_taunt(
        state,
        registry,
        &crate::combat::captain_attacker_id(player_id),
        target_instance_id,
        target_is_captain,
        false,
    )?;

    // Calculate ATK — the printed power of *this* attack plus the captain's
    // ATK modifiers (item 35; every character attack already reads `atk`).
    let mut atk = base_action.atk;
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
        || state.turn_number >= 7
        // Decision §8.23: a `haki` modifier on the captain itself.
        || crate::combat::attacker_haki_modifier(
            state,
            &crate::combat::captain_attacker_id(player_id),
        );
    let face_piercing = captain_has_trait_now(state, registry, player_id, Trait::Piercing)?;

    let attack_traits: Vec<AttackTrait> = base_action.attack_traits.clone().unwrap_or_default();

    // Get target DEF
    let mut target_def_val = captain_target_def(
        state,
        registry,
        player_id,
        target_instance_id,
        target_is_captain,
    )?;

    // Piercing (DEF / 2) — from the attack keywords or from the active face.
    // Decision §8.55: never applied to a negative DEF.
    if attack_traits.contains(&AttackTrait::Piercing) || face_piercing {
        target_def_val = target_def_val.max(0).div_euclid(2);
    }

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
        // Impact knocks the target back (the flag is only written when the
        // keyword is there, so the serialised attack is unchanged otherwise).
        pushback: attack_traits.contains(&AttackTrait::Impact).then_some(true),
        attack_traits,
        has_haki,
        ignore_shield: None,
        cannot_be_dodged: base_action.cannot_be_dodged,
        immobilize: base_action.immobilize,
        sleep: None,
        pushback_slots: None,
        strip_stealth: base_action.strip_stealth,
        survive_played: None,
        survive_target_id: None,
        damage_reduction: None,
        ignore_def: None,
        permanent_pv_loss: None,
        no_heal: None,
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

/// The error a frozen / immobilised / sleeping captain gets (item 35).
const CAPTAIN_CANNOT_ACT: &str = "Captain cannot act (frozen, immobilized or asleep)";

/// Decision §8.40 — the one place a captain's active traits are read:
/// `captain_traits(def, flipped) = def.traits ∪ (flipped ? verso.traits : [])`.
///
/// The card-level `CaptainDef.traits` belong to the captain on both faces (it
/// is where `CAP-LUFFY`'s `conqueror` lives), and the verso adds its own once
/// the captain is flipped onto the board. The TS engine read `verso.traits`
/// alone for Logia and the card-level list alone for the recto, so a top-level
/// keyword was invisible to the flipped face — this union is what every
/// caller (Logia, Cursed, Piercing, Rush, Conqueror) now uses.
///
/// Inert on the shipped catalogue: only `CAP-LUFFY` has a non-empty top-level
/// list (`conqueror`), which its verso repeats.
pub fn captain_traits(def: &crate::types::CaptainDef, flipped: bool) -> Vec<Trait> {
    let mut out: Vec<Trait> = def.traits.clone().unwrap_or_default();
    if flipped {
        for t in def.verso.traits.iter().flatten() {
            if !out.contains(t) {
                out.push(*t);
            }
        }
    }
    out
}

/// [`captain_traits`] membership test.
pub fn captain_has_trait(def: &crate::types::CaptainDef, flipped: bool, t: Trait) -> bool {
    captain_traits(def, flipped).contains(&t)
}

/// Decision §8.28 (follow-up) × §8.40 — the captain's *live* traits: its
/// printed [`captain_traits`] **plus** the traits its equipment grants.
///
/// This is [`crate::board::has_trait`] for a captain, and it exists for the
/// same reason: once the captain can wear the fruit printed for it
/// ("Équipable sur Luffy / Crocodile / Akainu"), that fruit's `grantsTraits` —
/// `logia` and `cursed` on `BW-011` / `MR-011`, `cursed` on `MG-014`, plus the
/// awakening list once awakened — belong to the captain exactly as they belong
/// to a character. Every reader of a captain trait (Logia intangibility,
/// Cursed for the Water element, Piercing, Rush, Conquérant) goes through
/// here; with no attachment it is [`captain_has_trait`] unchanged.
pub fn captain_has_trait_now(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    t: Trait,
) -> Result<bool, EngineError> {
    let captain = &state.players.get(player_id).captain;
    let def = registry.get_captain_def(&captain.def_id)?;
    if captain_has_trait(def, captain.flipped, t) {
        return Ok(true);
    }
    crate::board::attachments_grant_trait(state, registry, &captain.attached_objects, t)
}

/// The DEF a captain attack is computed against — the mirror of the private
/// `target_def_value` in `combat.rs` (the TS engine repeats this block in each
/// `declare*`): the opposing captain's active-face DEF plus its `def`
/// modifiers, or the target character's effective DEF.
fn captain_target_def(
    state: &GameState,
    registry: &CardRegistry,
    attacker_owner: PlayerId,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<i32, EngineError> {
    if !target_is_captain {
        return get_effective_def(state, registry, target_instance_id);
    }
    let opponent_id = attacker_owner.opponent();
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
    // Decision §8.55: DEF is a non-negative stat — a captain debuffed below 0
    // must not *gain* damage through the piercing `floor(def / 2)`.
    Ok(v.max(0))
}

/// The captain's signature move — §8.2 item 34(a).
///
/// Dispatched from the existing `captainAttack` action with
/// `isSpecial: Some(true)` (the field is already on the wire and in the UI
/// contract, so no new variant is introduced). It pays
/// `verso.specialAttack.cost`, taps the captain, sets both action flags and
/// builds the pending attack exactly like a character special:
/// `atkBonus`, `element`, `attackTraits` (+ `zone` for `twoTargets`),
/// `conditionalBonus`, piercing, `ignoreDef`, `ignoreShield`, `immobilize`,
/// `sleep`, `cannotBeDodged`, `pushback`, `stripStealth` and `oncePerGame`
/// (recorded under the attack name in `captain.used_once_abilities`).
///
/// `permanentPvLoss` (Crocodile's Desert Girasol, −2 PV) rides on
/// [`PendingAttack::permanent_pv_loss`] (decision §8.38) and is applied by
/// `resolve_attack` once the blow lands: the maximum PV drops for good and the
/// current PV follows it down. It is no longer folded into the raw damage,
/// where a counter could have reduced it away.
///
/// Errors: as [`declare_captain_base_attack`], plus
/// `Captain special already used`, `Already used this ability (1x/game)` and
/// `Cannot afford captain special (cost {n})`.
pub fn declare_captain_special_attack(
    state: &mut GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<(), EngineError> {
    let captain = &state.players.get(player_id).captain;
    let def = registry.get_captain_def(&captain.def_id)?;
    let spec = def.verso.special_attack.clone();
    declare_captain_spec_attack(
        state,
        registry,
        player_id,
        &spec,
        spec.once_per_game
            .unwrap_or(false)
            .then(|| spec.name.clone()),
        target_instance_id,
        target_is_captain,
    )
}

/// The `useSurcharge` action — §8.2 item 34(b).
///
/// Generic, data-driven plumbing for the `surcharge` block of the captain's
/// **active** face: it resolves through the same code path as
/// [`declare_captain_special_attack`] and costs `surcharge.cost`. Inert on the
/// shipped catalogue (every face has `surcharge: None`).
///
/// The active face is always the verso: the surcharge is an attack and the
/// recto captain cannot attack (Rulebook v3.1 §2.1, decision §8.34), so a
/// recto captain is refused before its face is read.
///
/// The one-per-turn limit falls out of the shared per-turn flags (`tapped` /
/// `usedSpecialAttack`, cleared by `reset_turn_flags`); the
/// `surcharge_{name}` key in `captain.used_once_abilities` — that list is never
/// cleared — guards a `oncePerGame` surcharge.
///
/// Errors: as [`declare_captain_special_attack`], plus
/// `Captain not flipped (verso required)` and `Captain face has no
/// surcharge`.
pub fn use_captain_surcharge(
    state: &mut GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<(), EngineError> {
    let captain = &state.players.get(player_id).captain;
    // Decision §8.34: the surcharge resolves through the captain *special
    // attack* path, and "Le Capitaine recto ne peut pas attaquer" (Rulebook
    // v3.1 §2.1, quoted verbatim in `captains.ts`). The active face is
    // therefore necessarily the verso: a recto captain is refused here with
    // the same message the shared body uses, instead of reading a
    // `recto.surcharge` that could never be played (that branch was dead code
    // — the shared body opened with this very check).
    if !captain.flipped {
        return Err(EngineError::illegal("Captain not flipped (verso required)"));
    }
    let def = registry.get_captain_def(&captain.def_id)?;
    let Some(surcharge) = def.verso.surcharge.clone() else {
        return Err(EngineError::illegal("Captain face has no surcharge"));
    };
    let once_key = surcharge
        .once_per_game
        .unwrap_or(false)
        .then(|| once_surcharge(&surcharge.name));
    declare_captain_spec_attack(
        state,
        registry,
        player_id,
        &surcharge,
        once_key,
        target_instance_id,
        target_is_captain,
    )
}

/// The shared body of [`declare_captain_special_attack`] and
/// [`use_captain_surcharge`] — a captain attack driven by a
/// [`SpecialAttack`] block.
fn declare_captain_spec_attack(
    state: &mut GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    spec: &SpecialAttack,
    once_key: Option<String>,
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
    if captain.used_special_attack {
        return Err(EngineError::illegal("Captain special already used"));
    }
    if captain_cannot_act(&captain.status_effects) {
        return Err(EngineError::illegal(CAPTAIN_CANNOT_ACT));
    }

    let def = registry.get_captain_def(&captain.def_id)?;

    // Captain summoning sickness — same rule as the base action.
    if captain.deployed_turn == Some(i64::from(state.turn_number))
        && !captain_has_trait_now(state, registry, player_id, Trait::Rush)?
    {
        return Err(EngineError::illegal("Captain has summoning sickness"));
    }

    if let Some(key) = once_key.as_deref()
        && captain.used_once(key)
    {
        return Err(EngineError::illegal("Already used this ability (1x/game)"));
    }
    if !state.can_afford(player_id, spec.cost) {
        return Err(EngineError::illegal(format!(
            "Cannot afford captain special (cost {})",
            spec.cost
        )));
    }

    // Decision §8.38 — the taunt binds the captain's special too (see
    // `declare_captain_base_attack`: inert while nothing can taunt a captain).
    crate::combat::enforce_taunt(
        state,
        registry,
        &crate::combat::captain_attacker_id(player_id),
        target_instance_id,
        target_is_captain,
        true,
    )?;

    let cap_name = def.name.clone();
    let face_piercing = captain_has_trait_now(state, registry, player_id, Trait::Piercing)?;
    let has_natural_haki = def
        .verso
        .natural_haki
        .as_ref()
        .is_some_and(|h| !h.is_empty());

    // Base ATK = the verso stat plus the captain's ATK modifiers, then the
    // attack's own bonus (the character path does exactly this).
    let mut base_atk = def.verso.atk;
    for m in &captain.modifiers {
        if m.stat == ModifierStat::Atk {
            base_atk += m.amount;
        }
    }
    let cond_bonus = conditional_atk_bonus(
        state,
        registry,
        spec.conditional_bonus.as_ref(),
        target_instance_id,
        target_is_captain,
        player_id.opponent(),
    )?;
    let total_atk = base_atk + spec.atk_bonus + cond_bonus;

    // "Touche 2 cibles" is approximated as a small Zone, like the character special.
    let mut attack_traits: Vec<AttackTrait> = spec.attack_traits.clone().unwrap_or_default();
    if spec.two_targets.unwrap_or(false) && !attack_traits.contains(&AttackTrait::Zone) {
        attack_traits.push(AttackTrait::Zone);
    }

    let mut target_def_val = captain_target_def(
        state,
        registry,
        player_id,
        target_instance_id,
        target_is_captain,
    )?;
    if attack_traits.contains(&AttackTrait::Piercing) || face_piercing {
        // Decision §8.55: DEF is clamped at 0 before the halving.
        target_def_val = target_def_val.max(0).div_euclid(2);
    }
    // TS truthiness: `ignoreDef: 0` is falsy — no clamp at all.
    if let Some(ignore) = spec.ignore_def.filter(|v| *v != 0) {
        target_def_val = (target_def_val - ignore).max(0);
    }

    // Decision §8.38: `permanentPvLoss` is a loss of *maximum* PV ("La cible
    // perd 2 PV permanent (Sable)"), carried on the pending attack and applied
    // by `resolve_attack` once the blow lands — it is no longer approximated
    // as extra raw damage, which a counter could have reduced away.
    let raw_damage = (total_atk - target_def_val).max(0);

    // Haki to pierce Logia: natural Haki, Armament passive (T7+), or Water element.
    let has_haki = has_natural_haki
        || state.turn_number >= 7
        || spec.element == Some(Element::Water)
        || state.players.get(player_id).has_haki_this_turn()
        // Decision §8.23: a `haki` modifier on the captain itself.
        || crate::combat::attacker_haki_modifier(state, &crate::combat::captain_attacker_id(player_id));

    state.spend_volonte(player_id, spec.cost)?;

    {
        let cap = &mut state.players.get_mut(player_id).captain;
        cap.tapped = true;
        // One action per turn (Rulebook v3.1 §2.2/§6): base OR special, never both.
        cap.used_special_attack = true;
        cap.used_base_action = true;
        if let Some(key) = once_key {
            cap.used_once_abilities.push(key);
        }
    }

    state.pending_attack = Some(PendingAttack {
        attacker_id: crate::combat::captain_attacker_id(player_id),
        target_id: target_instance_id.to_string(),
        target_is_captain,
        is_special: true,
        raw_damage,
        attack_power: Some(total_atk),
        element: spec.element,
        attack_traits,
        has_haki,
        ignore_shield: spec.ignore_shield,
        cannot_be_dodged: spec.cannot_be_dodged,
        immobilize: spec.immobilize,
        sleep: spec.sleep,
        pushback: Some(spec.pushback.unwrap_or(false) || spec.pushback_slots.unwrap_or(0) > 0),
        pushback_slots: None,
        strip_stealth: spec.strip_stealth,
        survive_played: None,
        survive_target_id: None,
        damage_reduction: None,
        ignore_def: spec.ignore_def,
        permanent_pv_loss: spec.permanent_pv_loss,
        no_heal: spec.no_heal,
    });

    let target_name = if target_is_captain {
        "Capitaine".to_string()
    } else {
        let target_def_id = state.get_card(target_instance_id)?.def_id.clone();
        registry.get_card_def(&target_def_id)?.name.clone()
    };
    state.add_log(
        player_id,
        format!(
            "Capitaine {cap_name} utilise {} sur {target_name} (ATK {total_atk} vs DEF {target_def_val} = {raw_damage} degats)",
            spec.name
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
            embargo_turns: None,
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

    // --- canFlipCaptain: `cost ?? 0` (item 6) ---

    #[test]
    fn a_flip_condition_without_a_cost_is_free_and_legal() {
        // §8.1 item 6 — the predicate used to answer `false` for a condition
        // with no `cost` while `flip_captain` happily performed it for 0.
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let reg = CardRegistry::from_sets([vec![]], [cap]);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        state.players.get_mut(P1).volonte = 0;
        assert!(can_flip_captain(&state, &reg, P1).unwrap());
        flip_captain(&mut state, &reg, &mut ctx, P1, Slot::V1).unwrap();
        assert!(state.players.get(P1).captain.flipped);
        assert_eq!(state.players.get(P1).volonte, 0);
    }

    #[test]
    fn an_unaffordable_cost_is_still_refused_by_both_sides() {
        let cap = captain_def(
            "C1",
            FlipCondition {
                cost: Some(9),
                ..Default::default()
            },
            EntryEffect::GrantSelfRush,
        );
        let reg = CardRegistry::from_sets([vec![]], [cap]);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        state.players.get_mut(P1).volonte = 2;
        assert!(!can_flip_captain(&state, &reg, P1).unwrap());
        assert_eq!(
            flip_captain(&mut state, &reg, &mut ctx, P1, Slot::V1),
            Err(EngineError::illegal("Cannot afford captain flip"))
        );
    }

    // --- the three unread free-flip clauses (item 33) ---

    /// `can_flip_captain` and `flip_captain` must always agree: flip with 0
    /// Volonté in hand and report what the flip cost.
    fn free_flip_agrees(
        state: &mut GameState,
        reg: &CardRegistry,
        expected: FreeFlipReason,
    ) -> bool {
        let mut ctx = EngineContext::seeded(1);
        let free = free_flip_reason(state, reg, P1).unwrap() == Some(expected);
        state.players.get_mut(P1).volonte = 0;
        let predicate = can_flip_captain(state, reg, P1).unwrap();
        let executed = flip_captain(state, reg, &mut ctx, P1, Slot::A3).is_ok();
        free && predicate && executed && state.players.get(P1).volonte == 0
    }

    #[test]
    fn akainu_flips_free_facing_a_cursed_enemy() {
        let mut cursed = character("CU", 1, 0, 5);
        cursed.traits = Some(vec![Trait::Cursed]);
        let mut reg = CardRegistry::from_sets([vec![cursed, character("X", 1, 0, 5)]], []);
        reg.register_captain(crate::cards::captains::captain_akainu());
        let mut state = blank_state("CAP-AKAINU", "CAP-AKAINU");

        // No Cursed enemy: the printed cost of 3 applies.
        put(&mut state, &reg, "X", P2, Slot::V1);
        assert_eq!(free_flip_reason(&state, &reg, P1).unwrap(), None);
        state.players.get_mut(P1).volonte = 2;
        assert!(!can_flip_captain(&state, &reg, P1).unwrap());
        state.players.get_mut(P1).volonte = 3;
        assert!(can_flip_captain(&state, &reg, P1).unwrap());

        // A Cursed enemy makes it free.
        put(&mut state, &reg, "CU", P2, Slot::V2);
        assert!(free_flip_agrees(
            &mut state,
            &reg,
            FreeFlipReason::EnemyCursed
        ));
    }

    #[test]
    fn akainu_also_reads_a_cursed_enemy_captain_face() {
        let reg = CardRegistry::from_sets(
            [vec![]],
            [
                crate::cards::captains::captain_akainu(),
                crate::cards::captains::captain_luffy(),
            ],
        );
        let mut state = blank_state("CAP-AKAINU", "CAP-LUFFY");
        // Luffy recto is not Cursed...
        assert_eq!(free_flip_reason(&state, &reg, P1).unwrap(), None);
        // ... his verso face is.
        state.players.get_mut(P2).captain.flipped = true;
        assert!(free_flip_agrees(
            &mut state,
            &reg,
            FreeFlipReason::EnemyCursed
        ));
    }

    #[test]
    fn a_card_level_cursed_captain_stays_cursed_once_it_flips() {
        // Decision §8.40: there is one definition of "the captain's active
        // traits" — `captain_traits` — and `freeIfEnemyCursed` uses it. The
        // exclusive if/else it used to carry made a card-level `cursed`
        // invisible the moment the enemy captain flipped, so the free-flip
        // clause and the Water element disagreed about the same face.
        let mut akainu = crate::cards::captains::captain_akainu();
        // A hypothetical opponent whose `cursed` lives on the card, not on the
        // verso — the shape `captain_traits` exists for.
        let mut cursed_cap = crate::cards::captains::captain_shanks();
        cursed_cap.traits = Some(vec![Trait::Cursed]);
        cursed_cap.verso.traits = Some(vec![Trait::Piercing]);
        akainu.id = "CAP-A".to_string();
        cursed_cap.id = "CAP-C".to_string();
        let reg = CardRegistry::from_sets([vec![]], [akainu, cursed_cap]);
        let mut state = blank_state("CAP-A", "CAP-C");

        // Recto: the card-level trait already counts.
        assert_eq!(
            free_flip_reason(&state, &reg, P1).unwrap(),
            Some(FreeFlipReason::EnemyCursed)
        );
        // Flipped: it still does — the verso *adds* traits, it never replaces
        // the card's own.
        state.players.get_mut(P2).captain.flipped = true;
        assert!(captain_has_trait(
            reg.get_captain_def("CAP-C").unwrap(),
            true,
            Trait::Cursed
        ));
        assert!(free_flip_agrees(
            &mut state,
            &reg,
            FreeFlipReason::EnemyCursed
        ));
    }

    #[test]
    fn a_recto_captain_can_never_play_its_surcharge() {
        // Decision §8.34(b) reads "the active face's surcharge", and the
        // active face of an *attacking* captain is always the verso: "Le
        // Capitaine recto ne peut pas attaquer" (Rulebook v3.1 §2.1). The
        // recto branch was dead code — the shared body opened with this very
        // refusal — so the rule is stated where the action starts.
        let mut cap = crate::cards::captains::captain_shanks();
        cap.id = "CAP-S2".to_string();
        cap.recto.surcharge = Some(SpecialAttack {
            name: "Surcharge recto".to_string(),
            cost: 1,
            atk_bonus: 2,
            ..SpecialAttack::default()
        });
        cap.verso.surcharge = Some(SpecialAttack {
            name: "Surcharge verso".to_string(),
            cost: 1,
            atk_bonus: 2,
            ..SpecialAttack::default()
        });
        let reg = CardRegistry::from_sets([vec![character("X", 1, 0, 5)]], [cap]);
        let mut state = blank_state("CAP-S2", "CAP-S2");
        let target = put(&mut state, &reg, "X", P2, Slot::V1);
        state.players.get_mut(P1).volonte = 5;

        assert_eq!(
            use_captain_surcharge(&mut state, &reg, P1, &target, false).unwrap_err(),
            EngineError::illegal("Captain not flipped (verso required)")
        );
        assert!(state.pending_attack.is_none());
        assert_eq!(state.players.get(P1).volonte, 5, "and it costs nothing");

        // Flipped, the verso surcharge is the one that plays.
        state.players.get_mut(P1).captain.flipped = true;
        state.players.get_mut(P1).captain.slot = Some(Slot::A3);
        state.players.get_mut(P1).captain.deployed_turn = None;
        use_captain_surcharge(&mut state, &reg, P1, &target, false).unwrap();
        assert!(state.pending_attack.is_some());
        assert_eq!(state.players.get(P1).volonte, 4);
    }

    #[test]
    fn crocodile_flips_free_with_three_allies() {
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 0, 5)]], []);
        reg.register_captain(crate::cards::captains::captain_crocodile());
        let mut state = blank_state("CAP-CROCODILE", "CAP-CROCODILE");
        put(&mut state, &reg, "X", P1, Slot::V1);
        put(&mut state, &reg, "X", P1, Slot::V2);

        // Two allies: the printed cost of 3 applies.
        assert_eq!(free_flip_reason(&state, &reg, P1).unwrap(), None);
        state.players.get_mut(P1).volonte = 2;
        assert!(!can_flip_captain(&state, &reg, P1).unwrap());
        state.players.get_mut(P1).volonte = 3;
        assert!(can_flip_captain(&state, &reg, P1).unwrap());

        put(&mut state, &reg, "X", P1, Slot::A1);
        assert!(free_flip_agrees(
            &mut state,
            &reg,
            FreeFlipReason::AlliesGte
        ));
    }

    #[test]
    fn shanks_flips_free_from_turn_seven() {
        let reg = CardRegistry::from_sets([vec![]], [crate::cards::captains::captain_shanks()]);
        let mut state = blank_state("CAP-SHANKS", "CAP-SHANKS");

        // Turn 6: the printed cost of 4 applies.
        state.turn_number = 6;
        assert_eq!(free_flip_reason(&state, &reg, P1).unwrap(), None);
        state.players.get_mut(P1).volonte = 3;
        assert!(!can_flip_captain(&state, &reg, P1).unwrap());
        state.players.get_mut(P1).volonte = 4;
        assert!(can_flip_captain(&state, &reg, P1).unwrap());

        state.turn_number = 7;
        assert!(free_flip_agrees(&mut state, &reg, FreeFlipReason::TurnGte));
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
    fn flip_into_a_lethal_face_still_resolves_the_entry_effect() {
        // §8.1 item 32 — this test previously asserted the opposite (the entry
        // effect was skipped): the captain *does* arrive on the board, so its
        // entry effect happens and the game is decided afterwards.
        //
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
        // Entry effect first...
        assert_eq!(last_log(&state), "Effet d'entree : pioche 1 carte(s)");
        assert_eq!(state.log.len(), 2);
        // ... then the game is decided.
        assert_eq!(state.winner, Some(P2));
    }

    #[test]
    fn a_lethal_flip_keeps_the_cost_spent() {
        let mut cap = captain_def(
            "C1",
            FlipCondition {
                cost: Some(3),
                ..Default::default()
            },
            EntryEffect::Draw { amount: 1 },
        );
        cap.verso.pv = 5;
        let reg = CardRegistry::from_sets([vec![]], [cap]);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        state.players.get_mut(P1).captain.current_pv = 4;
        flip_captain(&mut state, &reg, &mut ctx, P1, Slot::V1).unwrap();
        assert_eq!(state.players.get(P1).volonte, 7);
        assert_eq!(state.winner, Some(P2));
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

    /// Decision §8.36/§8.40 — this test used to assert `sand` added one point
    /// of ordinary damage ("permanent PV loss approximated"). It encoded the
    /// bug: `sand` is "-1 PV **permanent**", i.e. `pv_max_loss += 1` with the
    /// current PV following the maximum down, exactly as the Sand element,
    /// `EventEffect::DamageEnemies { sand }` and the captain arm already do.
    #[test]
    fn damage_single_cursed_bonus_replaces_amount_and_sand_is_a_permanent_loss() {
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
            sand: Some(true),      // -1 PV permanent, not +1 damage
        };
        resolve_entry_effect(&mut state, &reg, &mut ctx, P1, &effect).unwrap();
        // 3 damage (the replacement total), then the maximum drops to 19 —
        // 17 is already below it, so nothing is clamped.
        assert_eq!(state.cards[&c].current_pv, 20 - 3);
        assert_eq!(state.cards[&c].pv_max_loss, Some(1));
        assert_eq!(last_log(&state), "Effet d'entree : 3 degats a Char CU !");
    }

    /// The other half of the same rule: an **undamaged** target loses one
    /// point of current PV too, because the current PV follows the maximum.
    #[test]
    fn entry_sand_pulls_a_full_pv_target_down_with_its_maximum() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let mut reg = CardRegistry::from_sets([vec![character("PL", 1, 0, 20)]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        let c = put(&mut state, &reg, "PL", P2, Slot::V1);
        let effect = EntryEffect::DamageEnemies {
            amount: 0,
            target: EntryDamageTarget::Single,
            cursed_bonus: None,
            sand: Some(true),
        };
        resolve_entry_effect(&mut state, &reg, &mut ctx, P1, &effect).unwrap();
        assert_eq!(state.cards[&c].pv_max_loss, Some(1));
        assert_eq!(state.cards[&c].current_pv, 19);
        // And the loss is irrecoverable: a heal cannot climb back over it.
        crate::board::heal_unit(&mut state, &reg, &c, 5).unwrap();
        assert_eq!(state.cards[&c].current_pv, 19);
    }

    /// Decision §8.14 — `cursedBonus` stays a replacement total, but a
    /// `Some(0)` is a real value: the JS-falsy-zero fallback to `amount` is
    /// gone (this could not be expressed before).
    #[test]
    fn damage_single_cursed_bonus_of_zero_really_means_zero() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let mut cursed = character("CU", 1, 0, 20);
        cursed.traits = Some(vec![Trait::Cursed]);
        let plain = character("PL", 1, 0, 20);
        let mut reg = CardRegistry::from_sets([vec![cursed, plain]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let mut ctx = EngineContext::seeded(1);
        let c = put(&mut state, &reg, "CU", P2, Slot::V1);
        let effect = EntryEffect::DamageEnemies {
            amount: 9,
            target: EntryDamageTarget::Single,
            cursed_bonus: Some(0),
            sand: None,
        };
        resolve_entry_effect(&mut state, &reg, &mut ctx, P1, &effect).unwrap();
        assert_eq!(state.cards[&c].current_pv, 20);

        // A non-Cursed target still takes `amount`.
        let mut state = blank_state("C1", "C1");
        let p = put(&mut state, &reg, "PL", P2, Slot::V1);
        resolve_entry_effect(&mut state, &reg, &mut ctx, P1, &effect).unwrap();
        assert_eq!(state.cards[&p].current_pv, 11);
    }

    /// Decision §8.40 — the single captain-trait helper: the card-level list
    /// belongs to both faces, `verso.traits` only counts once flipped.
    #[test]
    fn captain_traits_unions_the_card_level_and_verso_lists() {
        let mut def = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        def.traits = Some(vec![Trait::Conqueror]);
        def.verso.traits = Some(vec![Trait::Logia, Trait::Conqueror]);

        assert_eq!(captain_traits(&def, false), vec![Trait::Conqueror]);
        assert!(captain_has_trait(&def, false, Trait::Conqueror));
        assert!(!captain_has_trait(&def, false, Trait::Logia));

        assert_eq!(
            captain_traits(&def, true),
            vec![Trait::Conqueror, Trait::Logia]
        );
        assert!(captain_has_trait(&def, true, Trait::Logia));
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
            // `cursedBonus` stays ignored on the captain branch; `sand` is
            // *not* (§8.36/§8.40 — the captain has a max-PV model now).
            sand: Some(true),
        };
        resolve_entry_effect(&mut state, &reg, &mut ctx, P1, &effect).unwrap();
        assert_eq!(state.players.get(P2).captain.current_pv, 17);
        assert_eq!(state.players.get(P2).captain.pv_max_loss, Some(1));
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

    /// Decision §8.40 — `rush` is read through the trait **union** on every
    /// path, so a card-level `traits: ["rush"]` (no `verso.traits` entry) lets
    /// the captain act on the flip turn through the base attack and the
    /// special alike, and the enumerator offers both.
    #[test]
    fn a_card_level_rush_clears_the_captains_summoning_sickness_on_every_path() {
        let mut cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        cap.traits = Some(vec![Trait::Rush]);
        cap.verso.traits = None;
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 2, 5)]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let t = put(&mut state, &reg, "X", P2, Slot::V1);
        {
            let c = &mut state.players.get_mut(P1).captain;
            c.flipped = true;
            c.slot = Some(Slot::V2);
            c.deployed_turn = Some(i64::from(state.turn_number));
        }

        // The enumerator offers the captain attack on the flip turn…
        let actions =
            crate::actions::get_valid_actions(&state, &reg, P1).expect("enumeration works");
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, crate::types::GameAction::CaptainAttack { .. }))
        );
        // …and the base attack accepts it (it used to read `verso.traits`
        // alone and refuse with "Captain has summoning sickness").
        declare_captain_base_attack(&mut state, &reg, P1, &t, false).unwrap();
        assert!(state.pending_attack.is_some());
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

    // --- declareCaptainBaseAttack: the ignored attack data (item 35) ---

    /// Put the captain on the board, flipped and ready to act.
    fn versoed(state: &mut GameState, player: PlayerId, slot: Slot) {
        let cap = &mut state.players.get_mut(player).captain;
        cap.flipped = true;
        cap.slot = Some(slot);
        cap.deployed_turn = Some(1);
    }

    #[test]
    fn captain_base_attack_uses_the_action_atk_not_the_face_atk() {
        // §8.1 item 35 — `baseAction.atk` is the power of *this* attack. The
        // four shipped captains print `baseAction.atk == verso.atk`, so only a
        // divergent fixture can show the difference.
        let mut cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        cap.verso.atk = 7;
        cap.verso.base_action.atk = 9;
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 2, 5)]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let t = put(&mut state, &reg, "X", P2, Slot::V1);
        versoed(&mut state, P1, Slot::A1);
        declare_captain_base_attack(&mut state, &reg, P1, &t, false).unwrap();
        let pa = state.pending_attack.clone().unwrap();
        assert_eq!(pa.attack_power, Some(9));
        assert_eq!(pa.raw_damage, 7); // 9 - DEF 2
    }

    #[test]
    fn a_frozen_immobilized_or_sleeping_captain_cannot_attack() {
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 2, 5)]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let t = put(&mut state, &reg, "X", P2, Slot::V1);
        versoed(&mut state, P1, Slot::A1);
        for effect_type in [
            StatusEffectType::Freeze,
            StatusEffectType::Immobilize,
            StatusEffectType::Sleep,
        ] {
            state.players.get_mut(P1).captain.status_effects = vec![StatusEffect {
                effect_type,
                turns_remaining: 1,
                damage_per_turn: 0,
                source: "test".into(),
            }];
            assert_eq!(
                declare_captain_base_attack(&mut state, &reg, P1, &t, false),
                Err(EngineError::illegal(CAPTAIN_CANNOT_ACT)),
                "{effect_type:?}"
            );
            assert_eq!(
                declare_captain_special_attack(&mut state, &reg, P1, &t, false),
                Err(EngineError::illegal(CAPTAIN_CANNOT_ACT)),
                "{effect_type:?}"
            );
            // ... and the enumerator never offers it either.
            assert!(
                !crate::actions::get_valid_actions(&state, &reg, P1)
                    .unwrap()
                    .iter()
                    .any(|a| matches!(a, crate::types::GameAction::CaptainAttack { .. })),
                "{effect_type:?}"
            );
        }
    }

    #[test]
    fn captain_base_attack_carries_the_action_keywords() {
        // impact → pushback, piercing → DEF / 2, plus element / cannotBeDodged
        // / immobilize / stripStealth (MR-011 style attack data).
        let mut cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        cap.verso.base_action = BaseAction {
            name: "Choc".into(),
            atk: 6,
            attack_traits: Some(vec![AttackTrait::Impact, AttackTrait::Piercing]),
            element: Some(Element::Ice),
            cannot_be_dodged: Some(true),
            immobilize: Some(true),
            strip_stealth: Some(true),
            ..Default::default()
        };
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 5, 9)]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let t = put(&mut state, &reg, "X", P2, Slot::V1);
        versoed(&mut state, P1, Slot::A1);
        declare_captain_base_attack(&mut state, &reg, P1, &t, false).unwrap();
        let pa = state.pending_attack.clone().unwrap();
        assert_eq!(pa.raw_damage, 4); // 6 - (DEF 5 / 2 = 2)
        assert_eq!(pa.pushback, Some(true));
        assert_eq!(pa.element, Some(Element::Ice));
        assert_eq!(pa.cannot_be_dodged, Some(true));
        assert_eq!(pa.immobilize, Some(true));
        assert_eq!(pa.strip_stealth, Some(true));
        assert_eq!(
            pa.attack_traits,
            vec![AttackTrait::Impact, AttackTrait::Piercing]
        );
    }

    #[test]
    fn a_plain_captain_base_attack_writes_no_extra_fields() {
        // Parity guard: the shipped captains have no keywords on their base
        // action, so the serialised pending attack must not gain any flag.
        let cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 2, 5)]], []);
        reg.register_captain(cap);
        let mut state = blank_state("C1", "C1");
        let t = put(&mut state, &reg, "X", P2, Slot::V1);
        versoed(&mut state, P1, Slot::A1);
        declare_captain_base_attack(&mut state, &reg, P1, &t, false).unwrap();
        let json = serde_json::to_string(state.pending_attack.as_ref().unwrap()).unwrap();
        for key in [
            "pushback",
            "cannotBeDodged",
            "immobilize",
            "stripStealth",
            "element",
        ] {
            assert!(!json.contains(key), "{key} in {json}");
        }
    }

    // --- captain special attacks (item 34a) ---

    /// Flip `player`'s captain onto the board and drop a DEF-2 dummy in front.
    fn special_fixture(cap: CaptainDef) -> (CardRegistry, GameState, String) {
        let mut reg = CardRegistry::from_sets([vec![character("X", 1, 2, 30)]], []);
        let cap_id = cap.id.clone();
        reg.register_captain(cap);
        let mut state = blank_state(&cap_id, &cap_id);
        let t = put(&mut state, &reg, "X", P2, Slot::V1);
        versoed(&mut state, P1, Slot::A1);
        (reg, state, t)
    }

    #[test]
    fn luffy_bazooka_costs_three_and_pushes_back() {
        let (reg, mut state, t) = special_fixture(crate::cards::captains::captain_luffy());
        declare_captain_special_attack(&mut state, &reg, P1, &t, false).unwrap();
        let pa = state.pending_attack.clone().unwrap();
        assert!(pa.is_special);
        assert_eq!(pa.attack_power, Some(10)); // verso 6 + atkBonus 4
        assert_eq!(pa.raw_damage, 8); // - DEF 2
        assert_eq!(pa.attack_traits, vec![AttackTrait::Impact]);
        assert_eq!(pa.pushback, Some(true));
        assert_eq!(state.players.get(P1).volonte, 7);
        // Tapped: no base attack on top of the special this turn.
        let cap = &state.players.get(P1).captain;
        assert!(cap.tapped && cap.used_base_action && cap.used_special_attack);
        assert_eq!(
            declare_captain_base_attack(&mut state, &reg, P1, &t, false),
            Err(EngineError::illegal("Captain is tapped"))
        );
        assert_eq!(
            last_log(&state),
            "Capitaine Monkey D. Luffy utilise Gomu Gomu no Bazooka sur Char X (ATK 10 vs DEF 2 = 8 degats)"
        );
    }

    #[test]
    fn akainu_ryusei_kazan_is_a_fire_zone_attack() {
        let (reg, mut state, t) = special_fixture(crate::cards::captains::captain_akainu());
        declare_captain_special_attack(&mut state, &reg, P1, &t, false).unwrap();
        let pa = state.pending_attack.clone().unwrap();
        assert_eq!(pa.attack_power, Some(12)); // verso 8 + 4
        assert_eq!(pa.raw_damage, 10);
        assert_eq!(pa.element, Some(Element::Fire));
        assert_eq!(pa.attack_traits, vec![AttackTrait::Zone]);
        assert_eq!(state.players.get(P1).volonte, 6);
    }

    #[test]
    fn crocodile_desert_girasol_carries_the_permanent_pv_loss() {
        let (reg, mut state, t) = special_fixture(crate::cards::captains::captain_crocodile());
        declare_captain_special_attack(&mut state, &reg, P1, &t, false).unwrap();
        let pa = state.pending_attack.clone().unwrap();
        assert_eq!(pa.attack_power, Some(11)); // verso 7 + 4
        // Decision §8.38 supersedes the item-34 approximation: the 2 permanent
        // PV are no longer folded into `rawDamage` (where a `reduceDamage`
        // counter could have erased them). They ride on the pending attack and
        // `resolve_attack` turns them into a real max-PV loss.
        assert_eq!(pa.raw_damage, 9); // 11 - DEF 2
        assert_eq!(pa.permanent_pv_loss, Some(2));
        assert_eq!(pa.element, Some(Element::Sand));
        assert_eq!(state.players.get(P1).volonte, 6);
    }

    #[test]
    fn shanks_divin_depart_ignores_def_and_shield() {
        let (reg, mut state, t) = special_fixture(crate::cards::captains::captain_shanks());
        declare_captain_special_attack(&mut state, &reg, P1, &t, false).unwrap();
        let pa = state.pending_attack.clone().unwrap();
        assert_eq!(pa.attack_power, Some(12)); // verso 8 + 4
        assert_eq!(pa.raw_damage, 12); // ignoreDef 99 wipes the DEF 2
        assert_eq!(pa.ignore_shield, Some(true));
        assert!(pa.has_haki); // naturalHaki: armament
        assert_eq!(state.players.get(P1).volonte, 6);
    }

    #[test]
    fn a_captain_special_is_refused_when_unaffordable_or_already_used() {
        let (reg, mut state, t) = special_fixture(crate::cards::captains::captain_shanks());
        state.players.get_mut(P1).volonte = 3;
        assert_eq!(
            declare_captain_special_attack(&mut state, &reg, P1, &t, false),
            Err(EngineError::illegal(
                "Cannot afford captain special (cost 4)"
            ))
        );
        assert_eq!(state.players.get(P1).volonte, 3);
        assert!(state.pending_attack.is_none());

        state.players.get_mut(P1).volonte = 9;
        declare_captain_special_attack(&mut state, &reg, P1, &t, false).unwrap();
        state.players.get_mut(P1).captain.tapped = false;
        assert_eq!(
            declare_captain_special_attack(&mut state, &reg, P1, &t, false),
            Err(EngineError::illegal("Captain special already used"))
        );
    }

    #[test]
    fn a_once_per_game_captain_special_never_fires_twice() {
        let mut cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        cap.verso.special_attack.once_per_game = Some(true);
        let (reg, mut state, t) = special_fixture(cap);
        declare_captain_special_attack(&mut state, &reg, P1, &t, false).unwrap();
        assert_eq!(state.players.get(P1).captain.used_once_abilities, vec!["s"]);

        // A fresh turn clears the per-turn flags — the 1x/game key does not.
        {
            let cap = &mut state.players.get_mut(P1).captain;
            cap.tapped = false;
            cap.used_base_action = false;
            cap.used_special_attack = false;
        }
        assert_eq!(
            declare_captain_special_attack(&mut state, &reg, P1, &t, false),
            Err(EngineError::illegal("Already used this ability (1x/game)"))
        );
        // ... and it is no longer offered.
        assert!(
            !crate::actions::get_valid_actions(&state, &reg, P1)
                .unwrap()
                .iter()
                .any(|a| matches!(
                    a,
                    crate::types::GameAction::CaptainAttack {
                        is_special: Some(true),
                        ..
                    }
                ))
        );
    }

    #[test]
    fn a_recto_captain_has_no_special_attack() {
        let (reg, mut state, t) = special_fixture(crate::cards::captains::captain_luffy());
        state.players.get_mut(P1).captain.flipped = false;
        assert_eq!(
            declare_captain_special_attack(&mut state, &reg, P1, &t, false),
            Err(EngineError::illegal("Captain not flipped (verso required)"))
        );
    }

    #[test]
    fn the_captain_attack_action_dispatches_on_is_special() {
        use crate::types::GameAction;
        let (reg, mut state, t) = special_fixture(crate::cards::captains::captain_luffy());
        let mut ctx = EngineContext::seeded(1);
        crate::execute::execute_action(
            &mut state,
            &reg,
            &mut ctx,
            &GameAction::CaptainAttack {
                target_instance_id: t.clone(),
                target_is_captain: None,
                is_special: Some(true),
            },
        )
        .unwrap();
        let pa = state.pending_attack.clone().unwrap();
        assert!(pa.is_special);
        assert_eq!(pa.raw_damage, 8);
        assert_eq!(state.players.get(P1).volonte, 7);
    }

    // --- surcharge (item 34b) ---

    fn surcharge_captain() -> CaptainDef {
        let mut cap = captain_def("C1", FlipCondition::default(), EntryEffect::GrantSelfRush);
        cap.verso.surcharge = Some(SpecialAttack {
            name: "Surcharge".into(),
            cost: 2,
            atk_bonus: 3,
            element: Some(Element::Thunder),
            ignore_shield: Some(true),
            ..Default::default()
        });
        cap
    }

    #[test]
    fn surcharge_is_not_offered_when_the_face_has_none() {
        use crate::types::GameAction;
        let (reg, state, t) = special_fixture(crate::cards::captains::captain_luffy());
        assert!(
            !crate::actions::get_valid_actions(&state, &reg, P1)
                .unwrap()
                .iter()
                .any(|a| matches!(a, GameAction::UseSurcharge { .. }))
        );
        let mut state = state;
        assert_eq!(
            use_captain_surcharge(&mut state, &reg, P1, &t, false),
            Err(EngineError::illegal("Captain face has no surcharge"))
        );
    }

    #[test]
    fn a_surcharge_resolves_like_a_special() {
        use crate::types::GameAction;
        let (reg, mut state, t) = special_fixture(surcharge_captain());
        assert!(
            crate::actions::get_valid_actions(&state, &reg, P1)
                .unwrap()
                .iter()
                .any(|a| matches!(a, GameAction::UseSurcharge { .. }))
        );
        let mut ctx = EngineContext::seeded(1);
        crate::execute::execute_action(
            &mut state,
            &reg,
            &mut ctx,
            &GameAction::UseSurcharge {
                target_instance_id: t.clone(),
                target_is_captain: None,
            },
        )
        .unwrap();
        let pa = state.pending_attack.clone().unwrap();
        assert!(pa.is_special);
        assert_eq!(pa.attack_power, Some(10)); // verso 7 + 3
        assert_eq!(pa.raw_damage, 8);
        assert_eq!(pa.element, Some(Element::Thunder));
        assert_eq!(pa.ignore_shield, Some(true));
        assert_eq!(state.players.get(P1).volonte, 8);
        // One captain action per turn: the surcharge taps it.
        assert!(state.players.get(P1).captain.tapped);
        assert!(
            !crate::actions::get_valid_actions(&state, &reg, P1)
                .unwrap()
                .iter()
                .any(|a| matches!(a, GameAction::UseSurcharge { .. }))
        );
    }

    #[test]
    fn the_new_captain_actions_round_trip_through_serde() {
        use crate::types::GameAction;
        use serde_json::json;

        let special = GameAction::CaptainAttack {
            target_instance_id: "t".into(),
            target_is_captain: Some(false),
            is_special: Some(true),
        };
        let v = serde_json::to_value(&special).unwrap();
        assert_eq!(
            v,
            json!({"type": "captainAttack", "targetInstanceId": "t", "targetIsCaptain": false, "isSpecial": true})
        );
        assert_eq!(v["type"], json!(special.type_name()));
        assert_eq!(serde_json::from_value::<GameAction>(v).unwrap(), special);

        let surcharge = GameAction::UseSurcharge {
            target_instance_id: "t".into(),
            target_is_captain: None,
        };
        let v = serde_json::to_value(&surcharge).unwrap();
        // `targetIsCaptain` is `Option` + default, so it is omitted when absent.
        assert_eq!(v, json!({"type": "useSurcharge", "targetInstanceId": "t"}));
        assert_eq!(v["type"], json!("useSurcharge"));
        assert_eq!(surcharge.type_name(), "useSurcharge");
        assert_eq!(serde_json::from_value::<GameAction>(v).unwrap(), surcharge);
    }
}

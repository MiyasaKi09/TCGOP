//! Action execution — port of the mutating half of `src/engine/turnManager.ts`
//! (`executeAction` and everything it dispatches to that has no home of its own).
//!
//! The read-only half (`getValidActions`) lives in [`crate::actions`].
//!
//! `executeAction` is the single entry point for every state mutation: it is a
//! pure dispatcher over `GameAction` that forwards to `board.rs`, `combat.rs`,
//! `captain.rs`, `haki.rs`, `fruits.rs` and the local helpers below. A state
//! that already has a `winner` is returned untouched.

#![allow(clippy::collapsible_if)]
// ^ The nested `if` / `if let` blocks in this module mirror the TypeScript
// source branch for branch (see the per-function `PORT:` references). Merging
// them into let-chains would break that 1:1 reading, which is the whole point
// of the port, so the lint is turned off for this file only.

use crate::board::{
    deploy_character, deploy_ship, equip_object, get_board_characters, get_effective_atk,
    get_effective_def, get_empty_slots, is_front_slot, move_character, remove_from_board,
};
use crate::captain::{declare_captain_base_attack, flip_captain};
use crate::combat::{
    apply_counter_cancel, apply_counter_reduce, apply_counter_survive, apply_shield_block,
    declare_base_attack, declare_fruit_special_attack, declare_special_attack, resolve_attack,
};
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::fruits::awaken_fruit;
use crate::haki::{use_king_haki, use_observation_haki};
use crate::passives::{apply_on_ko_effects, recalculate_passive_buffs};
use crate::registry::CardRegistry;
use crate::state::{CardInstance, GameState, LogEntry, transactional};
use crate::types::{
    AtkDefStat, BuffDuration, CardType, CounterEffect, DamageTarget, EventEffect, GameAction,
    HakiType, Modifier, ModifierDuration, ModifierStat, PassiveEffect, PlayerId, Slot,
    StatusEffect, StatusEffectType, Trait, Zone,
};

// ============================================================
// Small local helpers (no TS counterpart)
// ============================================================

/// TS `Object.values(player.board).filter(Boolean)` — occupant ids in slot
/// order, cloned so the caller can mutate the state while iterating.
fn board_ids(state: &GameState, player_id: PlayerId) -> Vec<String> {
    state
        .players
        .get(player_id)
        .board
        .instance_ids()
        .into_iter()
        .cloned()
        .collect()
}

/// TS `Object.entries(player.board)` restricted to occupied slots.
fn board_slots(state: &GameState, player_id: PlayerId) -> Vec<(Slot, String)> {
    state
        .players
        .get(player_id)
        .board
        .occupied()
        .into_iter()
        .map(|(s, id)| (s, id.clone()))
        .collect()
}

/// TS `EventEffect.stat` / `BaseAction` stats → [`ModifierStat`].
fn mod_stat(stat: AtkDefStat) -> ModifierStat {
    match stat {
        AtkDefStat::Atk => ModifierStat::Atk,
        AtkDefStat::Def => ModifierStat::Def,
    }
}

/// TS `effect.duration === "turn" ? "turn" : "permanent"`.
fn mod_duration(duration: BuffDuration) -> ModifierDuration {
    match duration {
        BuffDuration::Turn => ModifierDuration::Turn,
        BuffDuration::Permanent => ModifierDuration::Permanent,
    }
}

/// A `turn`/`permanent` stat modifier, TS object-literal order.
fn modifier(
    id: String,
    stat: ModifierStat,
    amount: i32,
    source: String,
    duration: ModifierDuration,
) -> Modifier {
    Modifier {
        id,
        stat,
        amount,
        source,
        duration,
        turns_remaining: None,
    }
}

/// A status effect, TS object-literal order.
fn status(
    effect_type: StatusEffectType,
    turns_remaining: i32,
    damage_per_turn: i32,
    source: String,
) -> StatusEffect {
    StatusEffect {
        effect_type,
        turns_remaining,
        damage_per_turn,
        source,
    }
}

/// JS `str.match(/(\d+)/)` → `parseInt(m[1])`: the first maximal run of ASCII
/// digits anywhere in `s`.
fn first_number(s: &str) -> Option<i32> {
    let b = s.as_bytes();
    let start = b.iter().position(|c| c.is_ascii_digit())?;
    let end = b[start..]
        .iter()
        .position(|c| !c.is_ascii_digit())
        .map(|i| start + i)
        .unwrap_or(b.len());
    s[start..end].parse::<i32>().ok()
}

/// JS `str.match(/(\d+)\s*<word>/)` → `parseInt(m[1])`.
///
/// Leftmost match: at every byte offset that starts a digit, take the maximal
/// digit run, skip whitespace, and require `word` to follow. (Backtracking the
/// greedy `\d+` can never help — the next character would be a digit, which
/// `\s*<word>` cannot match.)
fn number_before(s: &str, word: &str) -> Option<i32> {
    number_after_prefix(s, "", word)
}

/// JS `str.match(/\+(\d+)\s*<word>/)` when `prefix == "+"`, and
/// [`number_before`] when `prefix == ""`.
fn number_after_prefix(s: &str, prefix: &str, word: &str) -> Option<i32> {
    // Iterate char boundaries only: these descriptions are UTF-8 French text
    // ("dégâts", "Portée"), so byte-offset slicing would panic.
    for (start, _) in s.char_indices() {
        let rest = &s[start..];
        let Some(digits_at) = rest.strip_prefix(prefix) else {
            continue;
        };
        let db = digits_at.as_bytes();
        let n = db.iter().take_while(|c| c.is_ascii_digit()).count();
        if n == 0 {
            continue;
        }
        let num = &digits_at[..n];
        let after = digits_at[n..].trim_start_matches(char::is_whitespace);
        if after.starts_with(word) {
            return num.parse::<i32>().ok();
        }
    }
    None
}

// ============================================================
// The dispatcher
// ============================================================

/// TS `executeAction(state, action)` — `src/engine/turnManager.ts:45`.
///
/// Returns the state unchanged when `state.winner` is already set. Otherwise
/// dispatches on `action.type`; every arm uses `state.currentPlayer` as the
/// acting player except `baseAttack` / `specialAttack` / `fruitSpecialAttack`
/// / `playCounter` / `useShield` / `passCounter`, which derive the player from
/// the card or the pending attack. `targetIsCaptain` and `isSpecial` default to
/// `false` when absent.
///
/// Note the TS `captainAttack` arm ignores `action.isSpecial` entirely — it
/// always calls `declareCaptainBaseAttack`. Reproduce that.
///
/// `endTurn` is `endTurn(state)` **followed by** `startTurn(next)`
/// — see [`end_turn_and_start_turn`].
///
/// Like the TS original this is **all-or-nothing**: the TS function threads a
/// new immutable state through every step and only returns it once the last
/// step succeeded, so a `throw` leaves the caller's state untouched. The body
/// therefore runs inside [`transactional`], which restores the pre-call
/// `GameState` when it fails.
pub fn execute_action(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    action: &GameAction,
) -> Result<(), EngineError> {
    transactional(state, |state| {
        execute_action_inner(state, registry, ctx, action)
    })
}

/// Body of [`execute_action`], run inside [`transactional`].
fn execute_action_inner(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    action: &GameAction,
) -> Result<(), EngineError> {
    // Don't allow actions if game is over
    if state.winner.is_some() {
        return Ok(());
    }

    let current = state.current_player;

    match action {
        GameAction::DeployCharacter { instance_id, slot } => {
            deploy_character(state, registry, ctx, current, instance_id, *slot)
        }

        GameAction::EquipObject {
            object_instance_id,
            target_instance_id,
        } => equip_object(
            state,
            registry,
            current,
            object_instance_id,
            target_instance_id,
        ),

        GameAction::DeployShip { instance_id } => {
            deploy_ship(state, registry, ctx, current, instance_id)
        }

        GameAction::BaseAttack {
            attacker_instance_id,
            target_instance_id,
            target_is_captain,
        } => declare_base_attack(
            state,
            registry,
            attacker_instance_id,
            target_instance_id,
            target_is_captain.unwrap_or(false),
        ),

        GameAction::SpecialAttack {
            attacker_instance_id,
            target_instance_id,
            target_is_captain,
        } => declare_special_attack(
            state,
            registry,
            ctx,
            attacker_instance_id,
            target_instance_id,
            target_is_captain.unwrap_or(false),
        ),

        GameAction::BaseSupportAction {
            instance_id,
            target_instance_id,
        } => execute_support_action(
            state,
            registry,
            ctx,
            current,
            instance_id,
            target_instance_id.as_deref(),
        ),

        GameAction::PlayEvent { instance_id, .. } => {
            play_event(state, registry, ctx, current, instance_id)
        }

        GameAction::PlayCounter { instance_id } => play_counter(state, registry, instance_id),

        GameAction::UseShield {
            blocker_instance_id,
        } => apply_shield_block(state, registry, blocker_instance_id),

        // Resolve the pending attack without counter
        GameAction::PassCounter => resolve_attack(state, registry, ctx),

        GameAction::FlipCaptain { slot } => flip_captain(state, registry, ctx, current, *slot),

        GameAction::CaptainAttack {
            target_instance_id,
            target_is_captain,
            // The TS arm never reads `isSpecial`.
            ..
        } => declare_captain_base_attack(
            state,
            registry,
            current,
            target_instance_id,
            target_is_captain.unwrap_or(false),
        ),

        GameAction::ActivateShip { ship_instance_id } => {
            activate_ship_ability(state, registry, ctx, current, ship_instance_id)
        }

        GameAction::UseHaki {
            haki_type,
            target_instance_id,
        } => handle_haki(
            state,
            registry,
            ctx,
            current,
            *haki_type,
            target_instance_id.as_deref(),
        ),

        GameAction::MoveCharacter {
            instance_id,
            target_slot,
        } => move_character(state, current, instance_id, *target_slot),

        GameAction::AwakenFruit { fruit_instance_id } => {
            awaken_fruit(state, registry, current, fruit_instance_id)
        }

        GameAction::FruitSpecialAttack {
            attacker_instance_id,
            fruit_instance_id,
            target_instance_id,
            target_is_captain,
        } => declare_fruit_special_attack(
            state,
            registry,
            attacker_instance_id,
            fruit_instance_id,
            target_instance_id,
            target_is_captain.unwrap_or(false),
        ),

        GameAction::EndTurn => end_turn_and_start_turn(state, registry, ctx),
    }
}

/// TS `case "endTurn": const next = endTurn(state); return startTurn(next);`
/// — `src/engine/turnManager.ts:122`.
///
/// The two halves themselves are already ported as
/// [`GameState::end_turn`](crate::state::GameState::end_turn) and
/// [`GameState::start_turn`](crate::state::GameState::start_turn) in `state.rs`;
/// this is only the pairing that `executeAction` performs, kept here so
/// `execute.rs` owns the whole action surface.
pub fn end_turn_and_start_turn(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
) -> Result<(), EngineError> {
    state.end_turn(registry)?;
    state.start_turn(registry, ctx)
}

// ============================================================
// Events
// ============================================================

/// TS `playEvent(state, playerId, instanceId)` — `src/engine/turnManager.ts:142`.
///
/// Pays `def.cost`, runs [`resolve_event_effect`] **while the card is still in
/// hand**, then discards it and logs `"Joue {name}"` (so the effect's own logs
/// come first).
///
/// Errors: `Card not found`, `Not your card`, `Card not in hand`,
/// `Not an event card`, `Cannot afford {name}`.
///
/// Like the TS original this is **all-or-nothing**: the TS function threads a
/// new immutable state through every step and only returns it once the last
/// step succeeded, so a `throw` leaves the caller's state untouched. The body
/// therefore runs inside [`transactional`], which restores the pre-call
/// `GameState` when it fails.
pub fn play_event(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    instance_id: &str,
) -> Result<(), EngineError> {
    transactional(state, |state| {
        play_event_inner(state, registry, ctx, player_id, instance_id)
    })
}

/// Body of [`play_event`], run inside [`transactional`].
fn play_event_inner(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    instance_id: &str,
) -> Result<(), EngineError> {
    let Some(card) = state.cards.get(instance_id) else {
        return Err(EngineError::UnknownInstance(instance_id.to_string()));
    };
    if card.owner != player_id {
        return Err(EngineError::NotYourCard(instance_id.to_string()));
    }
    if card.zone != Zone::Hand {
        return Err(EngineError::WrongZone {
            instance_id: instance_id.to_string(),
            expected: "hand".to_string(),
        });
    }

    let def = registry.get_card_def(&card.def_id)?.clone();
    if def.card_type != CardType::Event {
        return Err(EngineError::WrongCardType {
            expected: "event".to_string(),
        });
    }

    if !state.can_afford(player_id, def.cost) {
        return Err(EngineError::illegal(format!("Cannot afford {}", def.name)));
    }

    state.spend_volonte(player_id, def.cost)?;

    // Resolve event effect
    if let Some(effect) = def.event_effect.as_ref() {
        resolve_event_effect(state, registry, ctx, player_id, effect, &def.name)?;
    }

    // Discard the event
    {
        let p = state.players.get_mut(player_id);
        p.hand.retain(|id| id != instance_id);
        if let Some(c) = state.cards.get_mut(instance_id) {
            c.zone = Zone::Graveyard;
        }
        state
            .players
            .get_mut(player_id)
            .graveyard
            .push(instance_id.to_string());
    }

    state.add_log(player_id, format!("Joue {}", def.name));
    Ok(())
}

/// TS `resolveEventEffect(state, playerId, effect, cardName)`
/// — `src/engine/turnManager.ts:179`.
///
/// One arm per [`EventEffect`] variant:
/// - `gainWill` → `volonte += amount` (uncapped, no log);
/// - `draw` → N draws, then `discard` oldest-first from the **front** of the
///   hand + `"Defausse automatique de {n} carte(s) (plus ancienne en main)."`;
/// - `healAlly` → only the `allAllies` branch exists (single-target heal is dead
///   code in TS); `min(pv + amount, def.pv)`, ignores `noHeal`;
/// - `buffAllies` → `` `event_{cardName}_{Date.now()}` `` modifier on every own
///   board character, `turn` or `permanent`;
/// - `damageEnemies` → `allFront` restricts to V1–V3, `allCursed` to Cursed;
///   `cursedBonus` **replaces** `amount` for Cursed, `sand` adds 1;
///   `destroyShips` graveyards the enemy ship; then [`sweep_kos`];
/// - `tutor` → first matching character in deck order to hand +
///   `"{cardName} : recherche un personnage."`;
/// - `deployTokens` → N × [`deploy_token`];
/// - `healAllBuff` → heal + `` `feast_{id}_{Date.now()}` `` turn ATK modifier,
///   skipping units with `noHeal` / `desiccation`;
/// - `debuffAllEnemies` → `` `intim_{id}_{Date.now()}` `` turn `-atk`, plus an
///   `immobilize` 2t on non-`immuneControl` enemies with effective DEF ≤
///   `immobilizeMaxDef` (quirk: the TS reads the DEF from the *pre-modifier*
///   `next`, not the draft — reproduce that);
/// - `grantHakiAll` → `hakiThisTurn = true`, optional `` `haki_{id}_{Date.now()}` ``
///   turn ATK modifier, log `"{cardName} : Haki de l'Armement ce tour !"`;
/// - `rally` → `rally_atk_` / `rally_def_` turn modifiers + heal, no log;
/// - `buffSingle` → requires `charKOedThisGame` when `requiresOwnKO`
///   (error `Flashback: aucun de vos personnages n'a été KO ce match`),
///   then `` `flashback_{Date.now()}` `` on the highest effective-ATK ally;
/// - `rushBuff` → `` `burst_{Date.now()}` `` turn ATK on the highest-ATK ally and
///   `deployedTurn = -1` (`None` here) to clear its summoning sickness;
/// - `dodgeAll` → no-op;
/// - `custom` → `execute3` / `execute4` (KO the highest-PV enemy at or below the
///   PV cap, log `"{name} est exécuté !"` for the **opponent**, then
///   [`sweep_kos`]), `coordinatedFire` (damage = number of own `marine`-tagged
///   board characters, dealt to the lowest effective-DEF enemy, log
///   `"Ordre de Tir : {n} dégâts coordonnés."`), everything else just logs
///   `"{cardName} : {description}"`.
pub fn resolve_event_effect(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    effect: &EventEffect,
    card_name: &str,
) -> Result<(), EngineError> {
    match effect {
        EventEffect::GainWill { amount } => {
            state.players.get_mut(player_id).volonte += amount;
        }

        EventEffect::Draw { amount, discard } => {
            for _ in 0..*amount {
                state.draw_card(player_id);
            }
            if let Some(discard) = discard.filter(|d| *d > 0) {
                for _ in 0..discard {
                    if state.players.get(player_id).hand.is_empty() {
                        break;
                    }
                    // discard oldest, not newest
                    let discarded = state.players.get_mut(player_id).hand.remove(0);
                    if let Some(c) = state.cards.get_mut(&discarded) {
                        c.zone = Zone::Graveyard;
                    }
                    state.players.get_mut(player_id).graveyard.push(discarded);
                }
                state.add_log(
                    player_id,
                    format!("Defausse automatique de {discard} carte(s) (plus ancienne en main)."),
                );
            }
        }

        EventEffect::HealAlly { amount, all_allies } => {
            if all_allies.unwrap_or(false) {
                for id in board_ids(state, player_id) {
                    let Some(card) = state.cards.get(&id) else {
                        continue;
                    };
                    let max_pv = registry.get_card_def(&card.def_id)?.pv;
                    let card = state.cards.get_mut(&id).expect("just read");
                    let healed = card.current_pv + amount;
                    card.current_pv = healed.min(max_pv.unwrap_or(healed));
                }
            }
        }

        EventEffect::BuffAllies {
            stat,
            amount,
            duration,
            ..
        } => {
            let id_str = format!("event_{card_name}_{}", ctx.now());
            for id in board_ids(state, player_id) {
                if let Some(card) = state.cards.get_mut(&id) {
                    card.modifiers.push(modifier(
                        id_str.clone(),
                        mod_stat(*stat),
                        *amount,
                        card_name.to_string(),
                        mod_duration(*duration),
                    ));
                }
            }
        }

        EventEffect::DamageEnemies {
            amount,
            target,
            cursed_bonus,
            sand,
            destroy_ships,
        } => {
            let opponent_id = player_id.opponent();
            for (slot_key, id) in board_slots(state, opponent_id) {
                if *target == DamageTarget::AllFront && !is_front_slot(slot_key) {
                    continue;
                }
                let Some(card) = state.cards.get(&id) else {
                    continue;
                };
                let cdef = registry.get_card_def(&card.def_id)?;
                let cursed = cdef.has_trait(Trait::Cursed);
                if *target == DamageTarget::AllCursed && !cursed {
                    continue;
                }
                let mut dmg = match cursed_bonus.filter(|_| cursed) {
                    Some(bonus) if bonus != 0 => bonus,
                    _ => *amount,
                };
                if sand.unwrap_or(false) {
                    dmg += 1; // permanent PV loss approximated
                }
                if let Some(card) = state.cards.get_mut(&id) {
                    card.current_pv -= dmg;
                }
            }
            // Buster Call also destroys enemy ships.
            if destroy_ships.unwrap_or(false) {
                if let Some(ship_id) = state.players.get(opponent_id).active_ship.clone() {
                    if let Some(c) = state.cards.get_mut(&ship_id) {
                        c.zone = Zone::Graveyard;
                    }
                    let opp = state.players.get_mut(opponent_id);
                    opp.graveyard.push(ship_id);
                    opp.active_ship = None;
                }
            }
            sweep_kos(state, registry, ctx, opponent_id, player_id)?;
        }

        EventEffect::Tutor {
            filter_tag,
            max_cost,
        } => {
            let filter_tag = filter_tag.as_deref().filter(|t| !t.is_empty());
            let deck: Vec<String> = state.players.get(player_id).deck.clone();
            let mut found: Option<usize> = None;
            for (i, id) in deck.iter().enumerate() {
                let Some(card) = state.cards.get(id) else {
                    continue;
                };
                let d = registry.get_card_def(&card.def_id)?;
                if d.card_type != CardType::Character {
                    continue;
                }
                if filter_tag.is_some_and(|tag| !d.has_tag(tag)) {
                    continue;
                }
                if max_cost.is_some_and(|mc| d.cost > mc) {
                    continue;
                }
                found = Some(i);
                break;
            }
            if let Some(idx) = found {
                let tut = state.players.get_mut(player_id).deck.remove(idx);
                if let Some(c) = state.cards.get_mut(&tut) {
                    c.zone = Zone::Hand;
                }
                state.players.get_mut(player_id).hand.push(tut);
            }
            state.add_log(player_id, format!("{card_name} : recherche un personnage."));
        }

        EventEffect::DeployTokens { token_id, count } => {
            for _ in 0..*count {
                deploy_token(state, registry, ctx, player_id, token_id, None)?;
            }
        }

        EventEffect::HealAllBuff { heal, atk } => {
            let now = ctx.now();
            for id in board_ids(state, player_id) {
                let Some(card) = state.cards.get(&id) else {
                    continue;
                };
                if card.has_status(StatusEffectType::NoHeal)
                    || card.has_status(StatusEffectType::Desiccation)
                {
                    continue;
                }
                let max_pv = registry.get_card_def(&card.def_id)?.pv;
                let card = state.cards.get_mut(&id).expect("just read");
                // TS: `Math.min(c.currentPv + heal, d.pv ?? c.currentPv)` — the
                // fallback is the *pre-heal* PV, so a PV-less def heals nothing.
                card.current_pv = (card.current_pv + heal).min(max_pv.unwrap_or(card.current_pv));
                card.modifiers.push(modifier(
                    format!("feast_{id}_{now}"),
                    ModifierStat::Atk,
                    *atk,
                    card_name.to_string(),
                    ModifierDuration::Turn,
                ));
            }
        }

        EventEffect::DebuffAllEnemies {
            atk,
            immobilize_max_def,
        } => {
            let opponent_id = player_id.opponent();
            let now = ctx.now();
            let ids = board_ids(state, opponent_id);
            // QUIRK: the TS calls `getEffectiveDef(next, slot)` on the state as
            // it was *before* this effect's modifiers were pushed.
            let mut pre_def: Vec<i32> = Vec::with_capacity(ids.len());
            for id in &ids {
                pre_def.push(get_effective_def(state, registry, id)?);
            }
            for (i, id) in ids.iter().enumerate() {
                if !state.cards.contains_key(id) {
                    continue;
                }
                let mut immobilize = false;
                if let Some(max_def) = immobilize_max_def {
                    let def_id = state.cards[id].def_id.clone();
                    let d = registry.get_card_def(&def_id)?;
                    let ctrl_immune = d.passive.as_ref().is_some_and(|p| {
                        p.effects
                            .iter()
                            .any(|e| matches!(e, PassiveEffect::ImmuneControl))
                    });
                    immobilize = !ctrl_immune && pre_def[i] <= *max_def;
                }
                let card = state.cards.get_mut(id).expect("just checked");
                card.modifiers.push(modifier(
                    format!("intim_{id}_{now}"),
                    ModifierStat::Atk,
                    -*atk,
                    card_name.to_string(),
                    ModifierDuration::Turn,
                ));
                if immobilize {
                    card.status_effects.push(status(
                        StatusEffectType::Immobilize,
                        2,
                        0,
                        card_name.to_string(),
                    ));
                }
            }
        }

        EventEffect::GrantHakiAll { atk } => {
            let now = ctx.now();
            state.players.get_mut(player_id).haki_this_turn = Some(true);
            if let Some(atk) = atk.filter(|a| *a != 0) {
                for id in board_ids(state, player_id) {
                    if let Some(card) = state.cards.get_mut(&id) {
                        card.modifiers.push(modifier(
                            format!("haki_{id}_{now}"),
                            ModifierStat::Atk,
                            atk,
                            card_name.to_string(),
                            ModifierDuration::Turn,
                        ));
                    }
                }
            }
            state.add_log(
                player_id,
                format!("{card_name} : Haki de l'Armement ce tour !"),
            );
        }

        EventEffect::Rally { atk, def, heal } => {
            // Nakama! — all your allies +ATK/+DEF this turn and heal.
            let now = ctx.now();
            for id in board_ids(state, player_id) {
                let Some(card) = state.cards.get(&id) else {
                    continue;
                };
                let max_pv = registry.get_card_def(&card.def_id)?.pv;
                let card = state.cards.get_mut(&id).expect("just read");
                card.modifiers.push(modifier(
                    format!("rally_atk_{id}_{now}"),
                    ModifierStat::Atk,
                    *atk,
                    card_name.to_string(),
                    ModifierDuration::Turn,
                ));
                card.modifiers.push(modifier(
                    format!("rally_def_{id}_{now}"),
                    ModifierStat::Def,
                    *def,
                    card_name.to_string(),
                    ModifierDuration::Turn,
                ));
                card.current_pv = (card.current_pv + heal).min(max_pv.unwrap_or(card.current_pv));
            }
        }

        EventEffect::BuffSingle {
            stat,
            amount,
            duration,
            requires_own_ko,
        } => {
            // Flashback — needs an own KO this game; buff one ally (strongest) permanently.
            if requires_own_ko.unwrap_or(false)
                && !state.players.get(player_id).char_koed_this_game()
            {
                return Err(EngineError::illegal(
                    "Flashback: aucun de vos personnages n'a été KO ce match",
                ));
            }
            if let Some(best_id) = strongest_ally(state, registry, player_id)? {
                if let Some(card) = state.cards.get_mut(&best_id) {
                    card.modifiers.push(modifier(
                        format!("flashback_{}", ctx.now()),
                        mod_stat(*stat),
                        *amount,
                        card_name.to_string(),
                        mod_duration(*duration),
                    ));
                }
            }
        }

        EventEffect::RushBuff { atk } => {
            // Coup de Burst — one ally gains Rush + ATK this turn and can attack immediately.
            if let Some(best_id) = strongest_ally(state, registry, player_id)? {
                let now = ctx.now();
                if let Some(card) = state.cards.get_mut(&best_id) {
                    card.modifiers.push(modifier(
                        format!("burst_{now}"),
                        ModifierStat::Atk,
                        *atk,
                        card_name.to_string(),
                        ModifierDuration::Turn,
                    ));
                    // TS `deployedTurn = -1`; `None` is likewise never equal to
                    // `turnNumber`, so summoning sickness clears identically.
                    card.deployed_turn = None;
                }
            }
        }

        EventEffect::DodgeAll => {}

        EventEffect::Custom { id, description } => {
            let opponent_id = player_id.opponent();
            if id == "execute3" || id == "execute4" {
                let max_pv = if id == "execute4" { 4 } else { 3 };
                let enemies: Vec<(String, i32)> = get_board_characters(state, opponent_id)
                    .into_iter()
                    .filter(|c| c.current_pv <= max_pv)
                    .map(|c| (c.instance_id.clone(), c.current_pv))
                    .collect();
                if let Some((target_id, _)) = enemies
                    .iter()
                    .cloned()
                    .reduce(|a, b| if b.1 > a.1 { b } else { a })
                {
                    let ko_def_id = state.get_card(&target_id)?.def_id.clone();
                    let name = registry.get_card_def(&ko_def_id)?.name.clone();
                    state.add_log(opponent_id, format!("{name} est exécuté !"));
                    if let Some(c) = state.cards.get_mut(&target_id) {
                        c.current_pv = 0;
                    }
                    sweep_kos(state, registry, ctx, opponent_id, player_id)?;
                }
            } else if id == "coordinatedFire" {
                let mut marines = 0i32;
                for c in get_board_characters(state, player_id) {
                    if registry.get_card_def(&c.def_id)?.has_tag("marine") {
                        marines += 1;
                    }
                }
                if marines > 0 {
                    let enemies: Vec<String> = get_board_characters(state, opponent_id)
                        .into_iter()
                        .map(|c| c.instance_id.clone())
                        .collect();
                    if !enemies.is_empty() {
                        let mut best: Option<(String, i32)> = None;
                        for id in enemies {
                            let d = get_effective_def(state, registry, &id)?;
                            best = match best {
                                Some((bid, bd)) if d >= bd => Some((bid, bd)),
                                Some(_) | None => Some((id, d)),
                            };
                        }
                        let target_id = best.expect("non-empty").0;
                        if let Some(c) = state.cards.get_mut(&target_id) {
                            c.current_pv -= marines;
                        }
                        state.add_log(
                            player_id,
                            format!("Ordre de Tir : {marines} dégâts coordonnés."),
                        );
                        sweep_kos(state, registry, ctx, opponent_id, player_id)?;
                    }
                }
            } else {
                // embargo / betrayal / noHeal2 — flavour-logged (not yet enforced).
                state.add_log(player_id, format!("{card_name} : {description}"));
            }
        }
    }

    Ok(())
}

/// TS `buffSingle` / `rushBuff` target pick: the own board character with the
/// highest effective ATK, first one on a tie (`best` starts at `-1`, so any
/// character wins over "none").
fn strongest_ally(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Result<Option<String>, EngineError> {
    let mut best_id: Option<String> = None;
    let mut best = -1;
    for id in board_ids(state, player_id) {
        let a = get_effective_atk(state, registry, &id)?;
        if a > best {
            best = a;
            best_id = Some(id);
        }
    }
    Ok(best_id)
}

// ============================================================
// Tokens / KO sweep
// ============================================================

/// TS `deployToken(state, playerId, tokenDefId, slot?)`
/// — `src/engine/turnManager.ts:457`.
///
/// Places a fresh token instance (cost-free, `currentPv = def.pv ?? 1`,
/// `deployedTurn = turnNumber`) into `slot` when it is free, otherwise into the
/// first empty slot in board order; a full board is a silent no-op. Logs
/// `"Déploie {name}."` and then runs `recalculate_passive_buffs(playerId)`.
pub fn deploy_token(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    token_def_id: &str,
    slot: Option<Slot>,
) -> Result<(), EngineError> {
    let empties = get_empty_slots(state, player_id);
    let target = match slot {
        Some(s) if !state.players.get(player_id).board.is_occupied(s) => Some(s),
        _ => empties.first().copied(),
    };
    let Some(target) = target else {
        return Ok(());
    };

    let def = registry.get_card_def(token_def_id)?;
    let name = def.name.clone();
    let pv = def.pv.unwrap_or(1);
    let id = ctx.generate_instance_id(token_def_id);

    let mut instance = CardInstance::new(id.clone(), token_def_id.to_string(), player_id, pv);
    instance.zone = Zone::Board;
    instance.slot = Some(target);
    instance.deployed_turn = Some(state.turn_number);
    state.cards.insert(id.clone(), instance);
    state.players.get_mut(player_id).board.set(target, Some(id));
    state.log.push(LogEntry {
        turn: state.turn_number,
        player: player_id,
        message: format!("Déploie {name}."),
    });

    recalculate_passive_buffs(state, registry, player_id)
}

/// TS `sweepKOs(state, victimPlayerId, killerPlayerId)`
/// — `src/engine/turnManager.ts:478`.
///
/// Walks `victimPlayerId`'s board in slot order and, for each character at
/// `currentPv <= 0`, logs `"{name} est KO !"` (player = the victim),
/// grants the +2 Vol KO bonus to the victim, removes it from the board and runs
/// `apply_on_ko_effects(victim, killer, koDefId)`.
pub fn sweep_kos(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    victim_player_id: PlayerId,
    killer_player_id: PlayerId,
) -> Result<(), EngineError> {
    // The TS `for (const slot of Object.values(next.players[victim].board))`
    // snapshots the board once, then re-reads `next.cards[slot]` each round.
    for id in board_ids(state, victim_player_id) {
        let Some(card) = state.cards.get(&id) else {
            continue;
        };
        if card.zone == Zone::Board && card.current_pv <= 0 {
            let ko_def_id = card.def_id.clone();
            let name = registry.get_card_def(&ko_def_id)?.name.clone();
            state.add_log(victim_player_id, format!("{name} est KO !"));
            state.grant_ko_bonus(victim_player_id);
            remove_from_board(state, registry, &id)?;
            apply_on_ko_effects(
                state,
                registry,
                ctx,
                victim_player_id,
                killer_player_id,
                &ko_def_id,
            )?;
        }
    }
    Ok(())
}

// ============================================================
// Counters
// ============================================================

/// TS `playCounter(state, instanceId)` — `src/engine/turnManager.ts:502`.
///
/// Routes on `def.counterEffect.type`: `reduceDamage` →
/// [`crate::combat::apply_counter_reduce`], `survive` →
/// [`crate::combat::apply_counter_survive`], `cancel` / `untargetable` →
/// [`crate::combat::apply_counter_cancel`].
///
/// Errors: `Counter not found`, `Not a counter card`, `No counter effect`.
pub fn play_counter(
    state: &mut GameState,
    registry: &CardRegistry,
    instance_id: &str,
) -> Result<(), EngineError> {
    let Some(card) = state.cards.get(instance_id) else {
        return Err(EngineError::illegal("Counter not found"));
    };

    let def = registry.get_card_def(&card.def_id)?;
    if def.card_type != CardType::Counter {
        return Err(EngineError::illegal("Not a counter card"));
    }

    let Some(counter_effect) = def.counter_effect.as_ref() else {
        return Err(EngineError::illegal("No counter effect"));
    };

    match counter_effect {
        CounterEffect::ReduceDamage { .. } => apply_counter_reduce(state, registry, instance_id),
        CounterEffect::Survive { .. } => apply_counter_survive(state, registry, instance_id),
        CounterEffect::Cancel { .. } | CounterEffect::Untargetable { .. } => {
            apply_counter_cancel(state, registry, instance_id)
        }
    }
}

// ============================================================
// Ship actives
// ============================================================

/// TS `activateShipAbility(state, playerId, shipInstanceId)`
/// — `src/engine/turnManager.ts:529`.
///
/// Pays `shipActive.cost`, records the 1x/game use, then dispatches — first by
/// ship id, then by lower-cased description sniffing (reproduce the exact
/// substring tests and the `/(\d+)/` style regexes):
/// - `MG-021` Thousand Sunny "Gaon Cannon": N damage (first number in the
///   description, default 5) to the lowest effective-DEF enemy in the front pool
///   (else any), log `"{ship} : Gaon Cannon — {n} dégâts !"` + [`sweep_kos`];
///   with no enemy character it hits the captain
///   (`"{ship} : Gaon Cannon — {n} dégâts au Capitaine !"`) + win check;
/// - `BW-019` Navire Baroque Works: two `TOK-AGENT` tokens +
///   `"{ship} : deux agents déployés !"`;
/// - `RH-017` Red Force: `hakiThisTurn = true` and `+1` turn ATK
///   (`` `redforce_{id}_{Date.now()}` ``) +
///   `"{ship} : Haki d'Armement et +1 ATK ce tour !"`;
/// - description containing `"deg."` **and** `"avant"`: `/(\d+)\s*deg/` damage
///   to enemy V1–V3 (no KO sweep) + `"{ship} active {active} !"`;
/// - description containing `"+"` and `"atk"`/`"def"`: `/\+(\d+)\s*atk/` and
///   `/\+(\d+)\s*def/` turn buffs on every own board character
///   (`` `ship_{activeName}_atk_{Date.now()}` ``) + `"{ship} active {active} !"`;
/// - fallback: just `"{ship} active {active} !"`.
///
/// Errors: `Ship not found`, `Ship has no active ability`,
/// `Ship ability already used (1x/game)`, `Cannot afford {name} (cost {n})`.
///
/// Like the TS original this is **all-or-nothing**: the TS function threads a
/// new immutable state through every step and only returns it once the last
/// step succeeded, so a `throw` leaves the caller's state untouched. The body
/// therefore runs inside [`transactional`], which restores the pre-call
/// `GameState` when it fails.
pub fn activate_ship_ability(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    ship_instance_id: &str,
) -> Result<(), EngineError> {
    transactional(state, |state| {
        activate_ship_ability_inner(state, registry, ctx, player_id, ship_instance_id)
    })
}

/// Body of [`activate_ship_ability`], run inside [`transactional`].
fn activate_ship_ability_inner(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    ship_instance_id: &str,
) -> Result<(), EngineError> {
    let Some(ship) = state.cards.get(ship_instance_id) else {
        return Err(EngineError::illegal("Ship not found"));
    };

    let def = registry.get_card_def(&ship.def_id)?.clone();
    let Some(active) = def.ship_active.clone() else {
        return Err(EngineError::illegal("Ship has no active ability"));
    };

    let once_per_game = active.once_per_game.unwrap_or(false);
    if once_per_game && ship.used_once(&active.name) {
        return Err(EngineError::illegal("Ship ability already used (1x/game)"));
    }

    if !state.can_afford(player_id, active.cost) {
        return Err(EngineError::CannotAfford {
            what: active.name.clone(),
            cost: active.cost,
            has: state.players.get(player_id).volonte,
        });
    }

    if active.cost > 0 {
        state.spend_volonte(player_id, active.cost)?;
    }

    if once_per_game {
        if let Some(c) = state.cards.get_mut(ship_instance_id) {
            c.used_once_abilities.push(active.name.clone());
        }
    }

    let desc = active.description.to_lowercase();

    // Gaon Cannon (Thousand Sunny): 5 damage to one enemy (Portée).
    if def.id == "MG-021" {
        let opponent_id = player_id.opponent();
        let dmg = first_number(&active.description).unwrap_or(5);
        let enemies: Vec<(String, Option<Slot>)> = get_board_characters(state, opponent_id)
            .into_iter()
            .map(|c| (c.instance_id.clone(), c.slot))
            .collect();
        let front: Vec<String> = enemies
            .iter()
            .filter(|(_, s)| s.is_some_and(is_front_slot))
            .map(|(id, _)| id.clone())
            .collect();
        let pool: Vec<String> = if !front.is_empty() {
            front
        } else {
            enemies.into_iter().map(|(id, _)| id).collect()
        };
        let mut target_id: Option<String> = None;
        for id in pool {
            let replace = match target_id.as_deref() {
                None => true,
                Some(cur) => {
                    get_effective_def(state, registry, &id)?
                        < get_effective_def(state, registry, cur)?
                }
            };
            if replace {
                target_id = Some(id);
            }
        }
        if let Some(tid) = target_id {
            if let Some(c) = state.cards.get_mut(&tid) {
                c.current_pv -= dmg;
            }
            state.add_log(
                player_id,
                format!("{} : Gaon Cannon — {dmg} dégâts !", def.name),
            );
            sweep_kos(state, registry, ctx, opponent_id, player_id)?;
        } else {
            state.players.get_mut(opponent_id).captain.current_pv -= dmg;
            state.add_log(
                player_id,
                format!("{} : Gaon Cannon — {dmg} dégâts au Capitaine !", def.name),
            );
            if let Some(w) = state.check_win_condition() {
                state.winner = Some(w);
            }
        }
        return Ok(());
    }

    // Navire Baroque Works: deploy two agent tokens.
    if def.id == "BW-019" {
        deploy_token(state, registry, ctx, player_id, "TOK-AGENT", None)?;
        deploy_token(state, registry, ctx, player_id, "TOK-AGENT", None)?;
        state.add_log(player_id, format!("{} : deux agents déployés !", def.name));
        return Ok(());
    }

    // Red Force: this turn your characters gain Haki + ATK.
    if def.id == "RH-017" {
        let now = ctx.now();
        state.players.get_mut(player_id).haki_this_turn = Some(true);
        for id in board_ids(state, player_id) {
            if let Some(c) = state.cards.get_mut(&id) {
                c.modifiers.push(modifier(
                    format!("redforce_{id}_{now}"),
                    ModifierStat::Atk,
                    1,
                    format!("ship_{}", def.id),
                    ModifierDuration::Turn,
                ));
            }
        }
        state.add_log(
            player_id,
            format!("{} : Haki d'Armement et +1 ATK ce tour !", def.name),
        );
        return Ok(());
    }

    // Damage to front line ("deg. a toute la Ligne Avant ennemie")
    if desc.contains("deg.") && desc.contains("avant") {
        let dmg = number_before(&active.description, "deg").unwrap_or(0);
        if dmg > 0 {
            let opponent_id = player_id.opponent();
            for slot_key in Slot::FRONT {
                let id = state.players.get(opponent_id).board.get(slot_key).cloned();
                if let Some(id) = id {
                    if let Some(c) = state.cards.get_mut(&id) {
                        c.current_pv -= dmg;
                    }
                }
            }
        }
        state.add_log(player_id, format!("{} active {} !", def.name, active.name));
        return Ok(());
    }

    // Buff all allies ("tous allies/Marines +X ATK +Y DEF")
    if desc.contains('+') && (desc.contains("atk") || desc.contains("def")) {
        let atk_buff = number_after_prefix(&desc, "+", "atk").unwrap_or(0);
        let def_buff = number_after_prefix(&desc, "+", "def").unwrap_or(0);
        let now = ctx.now();
        for id in board_ids(state, player_id) {
            if let Some(c) = state.cards.get_mut(&id) {
                if atk_buff > 0 {
                    c.modifiers.push(modifier(
                        format!("ship_{}_atk_{now}", active.name),
                        ModifierStat::Atk,
                        atk_buff,
                        format!("ship_{}", def.id),
                        ModifierDuration::Turn,
                    ));
                }
                if def_buff > 0 {
                    c.modifiers.push(modifier(
                        format!("ship_{}_def_{now}", active.name),
                        ModifierStat::Def,
                        def_buff,
                        format!("ship_{}", def.id),
                        ModifierDuration::Turn,
                    ));
                }
            }
        }
        state.add_log(player_id, format!("{} active {} !", def.name, active.name));
        return Ok(());
    }

    // Fallback: just log the activation
    state.add_log(player_id, format!("{} active {} !", def.name, active.name));
    Ok(())
}

// ============================================================
// Support base actions
// ============================================================

/// TS `executeSupportAction(state, playerId, instanceId, targetInstanceId?)`
/// — `src/engine/turnManager.ts:674`.
///
/// Taps the character and burns **both** action flags, then takes the *first*
/// matching branch of `def.baseAction`:
/// `scry` (sort the top N of the deck by ascending printed cost, log
/// `"{name} utilise {ba} : réorganise le dessus du deck."`) →
/// `bluff` (`loseAction` 2t, `"… : {target} a peur et perd sa prochaine action !"`) →
/// `buffAllyAtk` (`` `support_{defId}_{Date.now()}` `` turn ATK,
/// `"… : {target} +{n} ATK ce tour."`) →
/// `immobilize` (`immobilize` 2t, `"… : {target} est immobilise !"`) →
/// a `description` containing `"piege"`/`"Piege"` (permanent `trap` at 3
/// damage/turn, `"… : piege pose sur {target} !"`, error `Trap needs a target`) →
/// `healAmount` (`"… : +{n} PV a {target}"`) →
/// the fallback global `+1` turn ATK on every ally
/// (`` `support_{baName}_{Date.now()}` ``) with `"{name} utilise {ba} !"`.
///
/// Errors: `Card not found`, `Not your card`, `Action already used`,
/// `Not a support action`, `Trap needs a target`.
///
/// Like the TS original this is **all-or-nothing**: the TS function threads a
/// new immutable state through every step and only returns it once the last
/// step succeeded, so a `throw` leaves the caller's state untouched. The body
/// therefore runs inside [`transactional`], which restores the pre-call
/// `GameState` when it fails.
pub fn execute_support_action(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    instance_id: &str,
    target_instance_id: Option<&str>,
) -> Result<(), EngineError> {
    transactional(state, |state| {
        execute_support_action_inner(
            state,
            registry,
            ctx,
            player_id,
            instance_id,
            target_instance_id,
        )
    })
}

/// Body of [`execute_support_action`], run inside [`transactional`].
fn execute_support_action_inner(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    instance_id: &str,
    target_instance_id: Option<&str>,
) -> Result<(), EngineError> {
    let Some(card) = state.cards.get(instance_id) else {
        return Err(EngineError::UnknownInstance(instance_id.to_string()));
    };
    if card.owner != player_id {
        return Err(EngineError::NotYourCard(instance_id.to_string()));
    }
    if card.tapped || card.used_base_action {
        return Err(EngineError::illegal("Action already used"));
    }

    let def = registry.get_card_def(&card.def_id)?.clone();
    let Some(ba) = def
        .base_action
        .clone()
        .filter(|b| b.is_support.unwrap_or(false))
    else {
        return Err(EngineError::illegal("Not a support action"));
    };

    {
        let c = state.cards.get_mut(instance_id).expect("just read");
        c.tapped = true;
        // One action per turn (Rulebook v3.1 §2.2/§6): a support base spends the turn's action.
        c.used_base_action = true;
        c.used_special_attack = true;
    }

    // Scry / reorder (Nami Prévisions): reorder the top N — keep the cheapest on top.
    if let Some(scry) = ba.scry.filter(|n| *n != 0) {
        let deck_len = state.players.get(player_id).deck.len();
        let n = (scry.max(0) as usize).min(deck_len);
        let mut top: Vec<String> = state.players.get(player_id).deck[..n].to_vec();
        let mut costs: Vec<(String, i32)> = Vec::with_capacity(n);
        for id in &top {
            let def_id = state.get_card(id)?.def_id.clone();
            costs.push((id.clone(), registry.get_card_def(&def_id)?.cost));
        }
        // JS `Array.prototype.sort` is stable, and so is `sort_by_key`.
        costs.sort_by_key(|(_, c)| *c);
        top = costs.into_iter().map(|(id, _)| id).collect();
        let deck = &mut state.players.get_mut(player_id).deck;
        deck[..n].clone_from_slice(&top);
        state.add_log(
            player_id,
            format!(
                "{} utilise {} : réorganise le dessus du deck.",
                def.name, ba.name
            ),
        );
        return Ok(());
    }

    // Bluff (Usopp): an enemy of DEF <= 1 loses its next action.
    if let (true, Some(target)) = (ba.bluff.unwrap_or(false), target_instance_id) {
        let tn = registry
            .get_card_def(&state.get_card(target)?.def_id)?
            .name
            .clone();
        if let Some(t) = state.cards.get_mut(target) {
            t.status_effects.push(status(
                StatusEffectType::LoseAction,
                2,
                0,
                instance_id.to_string(),
            ));
        }
        state.add_log(
            player_id,
            format!(
                "{} utilise {} : {tn} a peur et perd sa prochaine action !",
                def.name, ba.name
            ),
        );
        return Ok(());
    }

    // Buff one ally (Brook Mélodie): +N ATK this turn.
    if let (Some(buff), Some(target)) = (ba.buff_ally_atk.filter(|n| *n != 0), target_instance_id) {
        let tn = registry
            .get_card_def(&state.get_card(target)?.def_id)?
            .name
            .clone();
        state.get_card_mut(target)?.modifiers.push(modifier(
            format!("support_{}_{}", def.id, ctx.now()),
            ModifierStat::Atk,
            buff,
            format!("support_{}", def.id),
            ModifierDuration::Turn,
        ));
        state.add_log(
            player_id,
            format!(
                "{} utilise {} : {tn} +{buff} ATK ce tour.",
                def.name, ba.name
            ),
        );
        return Ok(());
    }

    // Immobilize (Robin, Kuzan)
    if let (true, Some(target)) = (ba.immobilize.unwrap_or(false), target_instance_id) {
        let target_name = registry
            .get_card_def(&state.get_card(target)?.def_id)?
            .name
            .clone();
        if let Some(t) = state.cards.get_mut(target) {
            t.status_effects.push(status(
                StatusEffectType::Immobilize,
                // survives start-of-turn decrement, blocks for 1 turn
                2,
                0,
                instance_id.to_string(),
            ));
        }
        state.add_log(
            player_id,
            format!(
                "{} utilise {} : {target_name} est immobilise !",
                def.name, ba.name
            ),
        );
        return Ok(());
    }

    // Trap (Usopp)
    if ba
        .description
        .as_deref()
        .is_some_and(|d| d.contains("piege") || d.contains("Piege"))
    {
        let Some(target) = target_instance_id else {
            return Err(EngineError::illegal("Trap needs a target"));
        };
        let target_name = registry
            .get_card_def(&state.get_card(target)?.def_id)?
            .name
            .clone();
        if let Some(t) = state.cards.get_mut(target) {
            t.status_effects.push(status(
                StatusEffectType::Trap,
                // permanent until triggered
                -1,
                3,
                instance_id.to_string(),
            ));
        }
        state.add_log(
            player_id,
            format!(
                "{} utilise {} : piege pose sur {target_name} !",
                def.name, ba.name
            ),
        );
        return Ok(());
    }

    // Heal (Chopper, Marine soldier)
    if let (Some(heal), Some(target)) = (ba.heal_amount.filter(|n| *n != 0), target_instance_id) {
        let target_def = registry
            .get_card_def(&state.get_card(target)?.def_id)?
            .clone();
        if let Some(t) = state.cards.get_mut(target) {
            let healed = t.current_pv + heal;
            t.current_pv = healed.min(target_def.pv.unwrap_or(healed));
        }
        state.add_log(
            player_id,
            format!(
                "{} utilise {} : +{heal} PV a {}",
                def.name, ba.name, target_def.name
            ),
        );
        return Ok(());
    }

    // Global buff (Brook "New World", Sengoku "Commandement", Nami "Mirage Tempo")
    // Apply a generic +1 ATK buff to all allies for the turn as default
    let mod_id = format!("support_{}_{}", ba.name, ctx.now());
    for id in board_ids(state, player_id) {
        if let Some(c) = state.cards.get_mut(&id) {
            c.modifiers.push(modifier(
                mod_id.clone(),
                ModifierStat::Atk,
                1,
                format!("support_{}", def.id),
                ModifierDuration::Turn,
            ));
        }
    }
    state.add_log(player_id, format!("{} utilise {} !", def.name, ba.name));
    Ok(())
}

// ============================================================
// Haki routing
// ============================================================

/// TS `handleHaki(state, playerId, action)` — `src/engine/turnManager.ts:809`.
///
/// `observation` → [`crate::haki::use_observation_haki`], `king` →
/// [`crate::haki::use_king_haki`], `armament` → no-op (it is a passive from T7).
/// `action.targetInstanceId` is never read by the TS engine.
pub fn handle_haki(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    player_id: PlayerId,
    haki_type: HakiType,
    target_instance_id: Option<&str>,
) -> Result<(), EngineError> {
    // The TS `handleHaki` never reads `action.targetInstanceId`.
    let _ = target_instance_id;
    match haki_type {
        HakiType::Observation => use_observation_haki(state, player_id),
        HakiType::King => use_king_haki(state, registry, ctx, player_id),
        // Armament (T7+) is a passive in Rulebook v3.1 — no activation.
        HakiType::Armament => Ok(()),
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    use crate::cards::registry as card_registry;
    use crate::decks::{marines_deck, mugiwara_deck};
    use crate::state::create_game;
    use crate::types::Zone;

    /// A real game plus one extra instance of `def_id` in `player`'s hand.
    fn game_with_in_hand(
        def_id: &str,
        player: PlayerId,
    ) -> (GameState, CardRegistry, EngineContext) {
        let reg = card_registry();
        let mut ctx = EngineContext::seeded(11);
        let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
        let iid = ctx.generate_instance_id(def_id);
        let mut inst = CardInstance::new(iid.clone(), def_id.to_string(), player, 0);
        inst.zone = Zone::Hand;
        state.add_instance(inst);
        state.players.get_mut(player).hand.push(iid);
        state.current_player = player;
        state.phase = crate::types::Phase::Main;
        (state, reg, ctx)
    }

    #[test]
    fn a_rejected_flashback_spends_nothing_and_keeps_the_card_in_hand() {
        // MG-024 "Flashback : Promesse" is `requiresOwnKO`. TS `playEvent`
        // spends the Volonte into a *local* `next`, so the throw from
        // `resolveEventEffect` discards the whole thing.
        let p = PlayerId::Player1;
        let (mut state, reg, mut ctx) = game_with_in_hand("MG-024", p);
        state.players.get_mut(p).volonte = 5;
        assert!(!state.players.get(p).char_koed_this_game());
        let instance_id = state.players.get(p).hand.last().unwrap().clone();
        let before = state.clone();

        let err = play_event(&mut state, &reg, &mut ctx, p, &instance_id).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Flashback: aucun de vos personnages n'a été KO ce match"
        );
        assert_eq!(state, before);
        assert_eq!(state.players.get(p).volonte, 5);
        assert!(state.players.get(p).hand.contains(&instance_id));
        assert_eq!(state.card(&instance_id).unwrap().zone, Zone::Hand);

        // Repeating the illegal action cannot drain the pool either.
        for _ in 0..3 {
            assert!(play_event(&mut state, &reg, &mut ctx, p, &instance_id).is_err());
        }
        assert_eq!(state.players.get(p).volonte, 5);
    }

    #[test]
    fn a_rejected_trap_support_does_not_burn_the_characters_turn() {
        use crate::types::{BaseAction, CardDef, CardType, Faction, Rarity, Slot};

        let p = PlayerId::Player1;
        let mut reg = card_registry();
        let mut trap = CardDef::new(
            "T-TRAP",
            "Piegeur",
            CardType::Character,
            1,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        trap.pv = Some(3);
        trap.base_action = Some(BaseAction {
            name: "Piege".into(),
            description: Some("pose un piege".into()),
            is_support: Some(true),
            ..Default::default()
        });
        reg.register_card(trap);

        let mut ctx = EngineContext::seeded(3);
        let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
        state.current_player = p;
        state.phase = crate::types::Phase::Main;
        let iid = ctx.generate_instance_id("T-TRAP");
        let mut inst = CardInstance::new(iid.clone(), "T-TRAP".into(), p, 3);
        inst.zone = Zone::Board;
        inst.slot = Some(Slot::V1);
        inst.deployed_turn = Some(0);
        state.add_instance(inst);
        state
            .players
            .get_mut(p)
            .board
            .set(Slot::V1, Some(iid.clone()));
        let before = state.clone();

        // TS taps inside `produce` into a local `next`, and the missing-target
        // throw discards it — the character keeps its whole turn.
        let err = execute_support_action(&mut state, &reg, &mut ctx, p, &iid, None).unwrap_err();
        assert_eq!(err.to_string(), "Trap needs a target");
        assert_eq!(state, before);
        assert!(!state.card(&iid).unwrap().tapped);
        assert!(!state.card(&iid).unwrap().used_base_action);
        assert!(!state.card(&iid).unwrap().used_special_attack);
    }

    #[test]
    fn execute_action_is_all_or_nothing_like_the_pure_ts_dispatcher() {
        let p = PlayerId::Player1;
        let (mut state, reg, mut ctx) = game_with_in_hand("MG-024", p);
        state.players.get_mut(p).volonte = 5;
        let instance_id = state.players.get(p).hand.last().unwrap().clone();
        let before = state.clone();

        let action = GameAction::PlayEvent {
            instance_id,
            targets: None,
        };
        assert!(execute_action(&mut state, &reg, &mut ctx, &action).is_err());
        assert_eq!(state, before);
    }

    #[test]
    fn first_number_takes_the_first_maximal_digit_run() {
        assert_eq!(first_number("5 deg. a un ennemi"), Some(5));
        assert_eq!(first_number("Inflige 12 degats"), Some(12));
        assert_eq!(first_number("aucun chiffre"), None);
    }

    #[test]
    fn number_before_word_matches_leftmost_like_the_js_regex() {
        // /(\d+)\s*deg/
        assert_eq!(number_before("3 deg. a la Ligne Avant", "deg"), Some(3));
        assert_eq!(number_before("inflige 2deg. avant", "deg"), Some(2));
        // A leading number that is NOT followed by "deg" is skipped.
        assert_eq!(number_before("Cout 1 : 4 deg. avant", "deg"), Some(4));
        assert_eq!(number_before("pas de degats chiffres", "deg"), None);
        // Multi-byte French text before/after the number must not panic.
        assert_eq!(
            number_before("Épée : 3 deg. a toute la Ligne Avant ennemie", "deg"),
            Some(3)
        );
        assert_eq!(number_before("dégâts à la Ligne Avant", "deg"), None);
    }

    #[test]
    fn number_after_plus_prefix_matches_the_atk_def_regexes() {
        let desc = "tous allies +2 atk +1 def ce tour".to_lowercase();
        assert_eq!(number_after_prefix(&desc, "+", "atk"), Some(2));
        assert_eq!(number_after_prefix(&desc, "+", "def"), Some(1));
        assert_eq!(number_after_prefix(&desc, "+", "pv"), None);
        // No '+' → no match, even though a number precedes the word.
        assert_eq!(number_after_prefix("3 atk", "+", "atk"), None);
    }

    #[test]
    fn mod_stat_and_duration_map_one_to_one() {
        assert_eq!(mod_stat(AtkDefStat::Atk), ModifierStat::Atk);
        assert_eq!(mod_stat(AtkDefStat::Def), ModifierStat::Def);
        assert_eq!(mod_duration(BuffDuration::Turn), ModifierDuration::Turn);
        assert_eq!(
            mod_duration(BuffDuration::Permanent),
            ModifierDuration::Permanent
        );
    }
}

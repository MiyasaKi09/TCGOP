//! Action execution — port of the mutating half of `src/engine/turnManager.ts`
//! (`executeAction` and everything it dispatches to that has no home of its own).
//!
//! The read-only half (`getValidActions`) lives in [`crate::actions`].
//!
//! `executeAction` is the single entry point for every state mutation: it is a
//! pure dispatcher over `GameAction` that forwards to `board.rs`, `combat.rs`,
//! `captain.rs`, `haki.rs`, `fruits.rs` and the local helpers below. A state
//! that already has a `winner` is returned untouched.
//!
//! # Free-text parsing policy (decision §8.54)
//!
//! Three places in the engine still read a French `description` instead of a
//! structured field: [`activate_ship_ability`] (the `shipActive.description`
//! branches), the `"piege"` branch of [`execute_support_action`], and
//! `board.rs`'s `ship_passive`. The policy the §8 decisions follow is:
//!
//! * **(a)** where a §8 item needs *new* behaviour out of a text field, the
//!   card data moves to structured fields and the engine and the data change in
//!   the same commit — done for the three text-only support specials
//!   (§8.38: `taunt` / `cleanse` / `buffAllyAtk`);
//! * **(b)** everywhere else the existing lower-case / substring / regex logic
//!   is reproduced byte for byte, quirks included — see [`first_number`],
//!   [`number_before`] and [`number_after_prefix`], which are ports of
//!   `/(\d+)/`, `/(\d+)\s*<word>/` and `/\+(\d+)\s*<word>/`;
//! * **(c)** no decision adds a *new* free-text branch. §8.11 does not either:
//!   it repairs the existing front-line branch (the test and the extraction now
//!   read the same lower-cased string, both spellings of "deg."/"dég."), which
//!   is a fix inside branch (b), not a new parse.

#![allow(clippy::collapsible_if)]
// ^ The nested `if` / `if let` blocks in this module mirror the TypeScript
// source branch for branch (see the per-function `PORT:` references). Merging
// them into let-chains would break that 1:1 reading, which is the whole point
// of the port, so the lint is turned off for this file only.

use crate::board::{
    deploy_character, deploy_ship, equip_object, get_board_characters, get_effective_atk,
    get_effective_def, get_empty_slots, heal_unit, is_front_slot, is_slot_free, move_character,
    remove_from_board, reset_on_entry,
};
use crate::captain::{
    declare_captain_base_attack, declare_captain_special_attack, flip_captain,
    use_captain_surcharge,
};
use crate::combat::{
    apply_counter_cancel, apply_counter_reduce, apply_counter_survive, apply_shield_block,
    declare_base_attack, declare_fruit_special_attack, declare_special_attack, resolve_attack,
};
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::fruits::awaken_fruit;
use crate::haki::{use_king_haki, use_observation_haki};
use crate::passives::{
    apply_enemy_debuff_auras, apply_on_ko_effects, matches_filter, recalculate_passive_buffs,
};
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

/// Decision §8.37 — the refusal an embargoed player gets on `equipObject`.
pub const EMBARGO_EQUIP: &str = "Embargo: cannot equip an object this turn";
/// Decision §8.37 — the refusal an embargoed player gets on `deployShip`.
pub const EMBARGO_SHIP: &str = "Embargo: cannot deploy a ship this turn";

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
/// The TS `captainAttack` arm ignored `action.isSpecial` entirely and always
/// called `declareCaptainBaseAttack`; §8.2 item 34(a) makes `isSpecial: true`
/// declare the captain's special attack, and the new `useSurcharge` action
/// resolves the active face's `surcharge` block through the same path.
///
/// `endTurn` is `endTurn(state)` **followed by** `startTurn(next)`
/// — see [`end_turn_and_start_turn`].
///
/// Decision §8.44: once the arm has resolved, `checkWinCondition` runs and
/// commits its verdict to `state.winner`.
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
        execute_action_inner(state, registry, ctx, action)?;
        // Decision §8.44: every resolved action ends with a win check, so a
        // captain finished off by an effect that has no win check of its own
        // (or a deck-out) ends the game at once. `check_win_condition` hands
        // back an already-set `winner` first, so it never overwrites one.
        if let Some(w) = state.check_win_condition() {
            state.winner = Some(w);
        }
        Ok(())
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
            target_is_captain,
        } => {
            // Decision §8.37 (`embargo`, `MR-027`): "L'adversaire ne peut ni
            // équiper ni jouer de Navire à son prochain tour."
            if state.players.get(current).is_embargoed() {
                return Err(EngineError::illegal(EMBARGO_EQUIP));
            }
            equip_object(
                state,
                registry,
                current,
                object_instance_id,
                target_instance_id,
                target_is_captain.unwrap_or(false),
            )
        }

        GameAction::DeployShip { instance_id } => {
            // Decision §8.37 (`embargo`, `MR-027`).
            if state.players.get(current).is_embargoed() {
                return Err(EngineError::illegal(EMBARGO_SHIP));
            }
            deploy_ship(state, registry, ctx, current, instance_id)
        }

        GameAction::BaseAttack {
            attacker_instance_id,
            target_instance_id,
            target_is_captain,
        } => declare_base_attack(
            state,
            registry,
            ctx,
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

        // §8.2 item 34(a): `isSpecial` is now honoured — the TS arm dropped it
        // on the floor and always declared the base attack.
        GameAction::CaptainAttack {
            target_instance_id,
            target_is_captain,
            is_special,
        } => {
            if is_special.unwrap_or(false) {
                declare_captain_special_attack(
                    state,
                    registry,
                    current,
                    target_instance_id,
                    target_is_captain.unwrap_or(false),
                )
            } else {
                declare_captain_base_attack(
                    state,
                    registry,
                    current,
                    target_instance_id,
                    target_is_captain.unwrap_or(false),
                )
            }
        }

        // §8.2 item 34(b): the captain's `surcharge` block.
        GameAction::UseSurcharge {
            target_instance_id,
            target_is_captain,
        } => use_captain_surcharge(
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
        } => {
            // Decision §8.43: Observation is the *defender's* reaction, so it
            // is routed to the player who is being attacked while a
            // `pendingAttack` is open; King and Armament stay with the acting
            // player. `use_observation_haki` re-checks the caller, so a
            // hand-built action cannot slip past this.
            let actor = if *haki_type == HakiType::Observation && state.pending_attack.is_some() {
                current.opponent()
            } else {
                current
            };
            handle_haki(
                state,
                registry,
                ctx,
                actor,
                *haki_type,
                target_instance_id.as_deref(),
            )
        }

        GameAction::MoveCharacter {
            instance_id,
            target_slot,
        } => move_character(state, registry, current, instance_id, *target_slot),

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
    state.end_turn(registry, ctx)?;
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
/// - `healAlly` → `allAllies` heals every own board character; without it
///   (decision §8.36) the **most wounded** one is healed. Both go through
///   [`crate::board::heal_unit`] (decision §8.5), so `noHeal` / `desiccation`
///   are skipped and `pv_max_loss` caps the result;
/// - `buffAllies` → `` `event_{cardName}_{Date.now()}` `` modifier on every own
///   board character that satisfies `filter` (decision §8.36), `turn` or
///   `permanent`;
/// - `damageEnemies` → `allFront` restricts to V1–V3, `allCursed` to Cursed,
///   `single` hits one enemy ([`strongest_enemy`], decision §8.36);
///   `cursedBonus` **replaces** `amount` for Cursed, `sand` is a permanent
///   max-PV loss of 1 (decision §8.36); `destroyShips` graveyards the enemy
///   ship; then [`sweep_kos`];
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
/// - `dodgeAll` → no-op (decision §8.36: no card, no rules text);
/// - `custom` → exactly six ids (decision §8.37): `execute3` / `execute4` (KO
///   the highest-PV enemy at or below the PV cap, log `"{name} est exécuté !"`
///   for the **opponent**, then [`sweep_kos`]), `coordinatedFire` (damage =
///   number of own `marine`-tagged board characters, dealt to the lowest
///   effective-DEF enemy, log `"Ordre de Tir : {n} dégâts coordonnés."`),
///   `embargo` (`embargoTurns = 1` on the opponent), `noHeal2` (a 2-turn
///   `noHeal` on every enemy body) and `betrayal`
///   ([`take_control_for_the_turn`]); an unknown id keeps the flavour log
///   `"{cardName} : {description}"`, which the three implemented ones also
///   emit.
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
                    // Decision §8.5: the single heal path.
                    heal_unit(state, registry, &id, *amount)?;
                }
            } else if let Some(id) = most_wounded_ally(state, player_id) {
                // Decision §8.36: "Soigne N PV a un allie" — the TS single
                // branch was dead code; the most wounded own character (lowest
                // `currentPv`, first on a tie) is the natural reading, and
                // `heal_unit` still skips a `noHeal` / `desiccation` unit.
                heal_unit(state, registry, &id, *amount)?;
            }
        }

        EventEffect::BuffAllies {
            stat,
            amount,
            filter,
            duration,
        } => {
            let id_str = format!("event_{card_name}_{}", ctx.now());
            for id in board_ids(state, player_id) {
                // Decision §8.36: the `filter` the TS engine never read —
                // `MR-022` Justice Absolue buffs Marines only.
                let Some(def_id) = state.cards.get(&id).map(|c| c.def_id.clone()) else {
                    continue;
                };
                if !matches_filter(registry, &def_id, filter.as_ref())? {
                    continue;
                }
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
            if *target == DamageTarget::Single {
                // Decision §8.36: `single` is not `all`. The target is picked
                // with the engine's standing convention (the one
                // `EntryEffect.damageEnemies { target: "single" }` already
                // uses): the highest effective ATK of the enemy front pool,
                // else of the whole board, else the enemy captain.
                match strongest_enemy(state, registry, opponent_id)? {
                    Some(tid) => {
                        damage_enemy(state, registry, &tid, *amount, *cursed_bonus, *sand)?;
                    }
                    None => {
                        // No body to hit: the blow lands on the captain.
                        // Decision §8.40 gave the captain a max-PV model
                        // (`CaptainInstance::pv_max_loss`), so `sand` is the
                        // same permanent loss here as it is on a character.
                        // `cursedBonus` is deliberately left out of this arm,
                        // unchanged (§8.36 pinned it to the body pool).
                        state.players.get_mut(opponent_id).captain.current_pv -= *amount;
                        if sand.unwrap_or(false) {
                            crate::board::apply_captain_permanent_pv_loss(
                                state,
                                registry,
                                opponent_id,
                                1,
                            )?;
                        }
                    }
                }
            } else {
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
                    damage_enemy(state, registry, &id, *amount, *cursed_bonus, *sand)?;
                }
            }
            // Buster Call also destroys enemy ships.
            if destroy_ships.unwrap_or(false) {
                if let Some(ship_id) = state.players.get(opponent_id).active_ship.clone() {
                    // TS `draft.cards[opp.activeShip].zone = "graveyard"` —
                    // unguarded, so a dangling ship id throws.
                    state.get_card_mut(&ship_id)?.zone = Zone::Graveyard;
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
                // TS `getCardDef(draft.cards[id].defId)` — unguarded, so a deck
                // entry with no instance throws out of the whole `playEvent`.
                let card = state.get_card(id)?;
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
                // TS `turnManager.ts:315` skips the *whole* entry (heal and
                // buff) for a `noHeal` / `desiccation` unit — parity kept.
                if card.has_status(StatusEffectType::NoHeal)
                    || card.has_status(StatusEffectType::Desiccation)
                {
                    continue;
                }
                // Decision §8.5: the single heal path.
                heal_unit(state, registry, &id, *heal)?;
                let card = state.cards.get_mut(&id).expect("just read");
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
            // it was *before* this effect's modifiers were pushed — hence the
            // snapshot. It is taken (and the lookup only ever runs) inside the
            // `effect.immobilizeMaxDef !== undefined` guard, so an effect
            // without that field never calls `getEffectiveDef` and can never
            // fail the way it does.
            let pre_state = immobilize_max_def.is_some().then(|| state.clone());
            for id in ids.iter() {
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
                    let pre = pre_state
                        .as_ref()
                        .expect("snapshot taken whenever immobilizeMaxDef is set");
                    immobilize = !ctrl_immune && get_effective_def(pre, registry, id)? <= *max_def;
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
                if !state.cards.contains_key(&id) {
                    continue;
                }
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
                // Decision §8.5: the single heal path.
                heal_unit(state, registry, &id, *heal)?;
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
                    // TS `c.deployedTurn = -1;` — clear summoning sickness so it
                    // can act now. The sentinel is stored as-is so the
                    // serialised `CardInstance` keeps the `deployedTurn` key.
                    card.deployed_turn = Some(-1);
                }
            }
        }

        // Decision §8.36: `dodgeAll` stays an explicit no-op. No card in the
        // catalogue carries it and the rulebook has no "everything dodges"
        // state, so inventing durable dodge state (and the wire fields it
        // would need) is not justified.
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
                        // TS `enemies.reduce((a, b) => ...)` with no seed: the
                        // callback runs only from the *second* element on, so a
                        // lone enemy is returned without `getEffectiveDef` ever
                        // being called (and so without it being able to throw).
                        // Keep the lookups lazy, exactly like Gaon Cannon below.
                        let mut best: Option<String> = None;
                        for id in enemies {
                            let replace = match best.as_deref() {
                                None => true,
                                Some(cur) => {
                                    get_effective_def(state, registry, &id)?
                                        < get_effective_def(state, registry, cur)?
                                }
                            };
                            if replace {
                                best = Some(id);
                            }
                        }
                        let target_id = best.expect("non-empty");
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
            } else if id == "embargo" {
                // Decision §8.37 — `MR-027`: "L'adversaire ne peut ni équiper
                // ni jouer de Navire à son prochain tour." One turn, theirs.
                state.players.get_mut(opponent_id).embargo_turns = Some(1);
                state.add_log(player_id, format!("{card_name} : {description}"));
            } else if id == "noHeal2" {
                // Decision §8.37 — `BW-028`: "Persistant (2 tours) : les
                // ennemis ne peuvent pas être soignés." Every heal path already
                // skips a `noHeal` unit (decision §8.5). A status only counts
                // down on its bearer's own turns, so `turnsRemaining: 2` covers
                // the rest of this turn plus the enemy's next one and lapses at
                // the start of the one after — the same reading every other
                // two-turn status in the engine has. A unit already under the
                // rain has its counter refreshed rather than stacked.
                for id in board_ids(state, opponent_id) {
                    let Some(card) = state.cards.get_mut(&id) else {
                        continue;
                    };
                    match card
                        .status_effects
                        .iter_mut()
                        .find(|e| e.effect_type == StatusEffectType::NoHeal)
                    {
                        Some(existing) => {
                            existing.turns_remaining = existing.turns_remaining.max(2);
                        }
                        None => card.status_effects.push(status(
                            StatusEffectType::NoHeal,
                            2,
                            0,
                            card_name.to_string(),
                        )),
                    }
                }
                state.add_log(player_id, format!("{card_name} : {description}"));
            } else if id == "betrayal" {
                // Decision §8.37 — `BW-024`: "Prenez le contrôle d'un ennemi de
                // coût ≤ 2 ce tour." The UI never sends `targets`, so the
                // engine's standing convention picks the strongest legal enemy.
                take_control_for_the_turn(state, registry, player_id, card_name)?;
                state.add_log(player_id, format!("{card_name} : {description}"));
            } else {
                // Decision §8.37: an id outside the six legal ones keeps the
                // flavour log — no id is invented.
                state.add_log(player_id, format!("{card_name} : {description}"));
            }
        }
    }

    Ok(())
}

/// Decision §8.36 — one `damageEnemies` hit on one board character:
/// `cursedBonus` **replaces** `amount` on a Cursed target (a bonus of 0 is
/// falsy in the TS and falls back to `amount`), and `sand` is a **permanent**
/// max-PV loss of 1 on top of the damage ("Sable : PV permanents"), not the
/// extra point of damage the port approximated it with.
fn damage_enemy(
    state: &mut GameState,
    registry: &CardRegistry,
    instance_id: &str,
    amount: i32,
    cursed_bonus: Option<i32>,
    sand: Option<bool>,
) -> Result<(), EngineError> {
    let Some(card) = state.cards.get(instance_id) else {
        return Ok(());
    };
    let cursed = registry
        .get_card_def(&card.def_id)?
        .has_trait(Trait::Cursed);
    // Decision §8.14: `cursedBonus` is a *replacement* total ("2 degats, 3
    // contre Maudit" — MG-025 Tempete reads 2 -> 3, never 2 + 3), but the
    // JS-falsy-zero quirk is gone: a `Some(0)` really means 0 damage against a
    // Cursed target instead of falling back to `amount`.
    let dmg = match cursed_bonus.filter(|_| cursed) {
        Some(bonus) => bonus,
        None => amount,
    };
    let Some(card) = state.cards.get_mut(instance_id) else {
        return Ok(());
    };
    card.current_pv -= dmg;
    if sand.unwrap_or(false) {
        // The current PV follows the maximum down, exactly like every other
        // permanent-loss path (`resolve_attack`'s `permanentPvLoss`, the Sand
        // element on a character and on a captain): a full-PV unit hit by
        // `BW-023` Tempete de Sable must not be left standing above its own
        // maximum until something else happens to touch it.
        crate::board::apply_permanent_pv_loss(state, registry, instance_id, 1)?;
    }
    Ok(())
}

/// Decision §8.36 — the engine's standing "one enemy" pick, shared with
/// `EntryEffect.damageEnemies { target: "single" }` (`captain.rs`): the
/// highest effective ATK of the enemy front pool, else of the whole enemy
/// board, else `None` (nobody but the captain is left).
fn strongest_enemy(
    state: &GameState,
    registry: &CardRegistry,
    opponent_id: PlayerId,
) -> Result<Option<String>, EngineError> {
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
    let mut best_id: Option<String> = None;
    for id in pool {
        let take = match best_id.as_deref() {
            None => true,
            Some(cur) => {
                get_effective_atk(state, registry, &id)? > get_effective_atk(state, registry, cur)?
            }
        };
        if take {
            best_id = Some(id);
        }
    }
    Ok(best_id)
}

/// Decision §8.37 (`betrayal`, `BW-024`) — borrow the enemy character with the
/// highest effective ATK whose **printed** cost is `<= 2` until the end of this
/// turn.
///
/// The body moves into the first free slot of the borrower's board with
/// `controlledBy = borrower` and `loanReturnSlot = <the slot it came from>`,
/// untapped and with both action flags cleared, so it can act at once for its
/// new side. `owner` is deliberately **not** touched: KO bonuses, graveyards
/// and the win condition keep pointing at the player who paid for the card.
/// [`GameState::end_turn`] walks it home.
///
/// A full board (no free slot) or no legal enemy leaves the state untouched
/// apart from the event's own flavour log.
fn take_control_for_the_turn(
    state: &mut GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    card_name: &str,
) -> Result<(), EngineError> {
    let opponent_id = player_id.opponent();
    let candidates: Vec<String> = get_board_characters(state, opponent_id)
        .into_iter()
        .map(|c| c.instance_id.clone())
        .collect();

    let mut best: Option<String> = None;
    for id in candidates {
        let def_id = state.get_card(&id)?.def_id.clone();
        if registry.get_card_def(&def_id)?.cost > 2 {
            continue;
        }
        let take = match best.as_deref() {
            None => true,
            Some(cur) => {
                get_effective_atk(state, registry, &id)? > get_effective_atk(state, registry, cur)?
            }
        };
        if take {
            best = Some(id);
        }
    }
    let Some(target_id) = best else {
        return Ok(());
    };
    let Some(home) = state.get_card(&target_id)?.slot else {
        return Ok(());
    };
    let Some(dest) = get_empty_slots(state, player_id).first().copied() else {
        return Ok(());
    };

    state.players.get_mut(opponent_id).board.set(home, None);
    state
        .players
        .get_mut(player_id)
        .board
        .set(dest, Some(target_id.clone()));
    let card = state.get_card_mut(&target_id)?;
    card.slot = Some(dest);
    card.controlled_by = Some(player_id);
    card.loan_return_slot = Some(home);
    card.tapped = false;
    card.used_base_action = false;
    card.used_special_attack = false;
    // Decision §8.29: the equipment travels with its bearer — the loan moves
    // the body to another cell (of another board), so its attachments follow.
    let attached = card.attached_objects.clone();
    crate::board::move_attached_objects(state, &attached, dest);

    let def_id = state.get_card(&target_id)?.def_id.clone();
    let name = registry.get_card_def(&def_id)?.name.clone();
    state.add_log(player_id, format!("{card_name} : {name} change de camp !"));

    recalculate_passive_buffs(state, registry, player_id)?;
    recalculate_passive_buffs(state, registry, opponent_id)?;
    apply_enemy_debuff_auras(state, registry)?;
    Ok(())
}

/// Decision §8.36 — the `healAlly` (single) pick: the own board character with
/// the lowest `currentPv`, the first one on a tie.
fn most_wounded_ally(state: &GameState, player_id: PlayerId) -> Option<String> {
    let mut best: Option<(String, i32)> = None;
    for id in board_ids(state, player_id) {
        let Some(card) = state.cards.get(&id) else {
            continue;
        };
        let pv = card.current_pv;
        if best.as_ref().is_none_or(|(_, b)| pv < *b) {
            best = Some((id, pv));
        }
    }
    best.map(|(id, _)| id)
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
    // Decision §8.31: the flipped captain's slot counts as occupied.
    let target = match slot {
        Some(s) if is_slot_free(state, player_id, s) => Some(s),
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
    // Decision §8.30: a body entering the board starts clean.
    reset_on_entry(&mut instance);
    instance.zone = Zone::Board;
    instance.slot = Some(target);
    instance.deployed_turn = Some(i64::from(state.turn_number));
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
/// - description containing `"deg."`/`"dég."` **and** `"avant"`:
///   `/(\d+)\s*deg/` (then `dég`) damage to enemy V1–V3, followed by
///   [`sweep_kos`] (decision §8.11) + `"{ship} active {active} !"`;
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

    // Damage to front line ("N deg. a toute la Ligne Avant ennemie")
    //
    // Decision §8.11: the test and the extraction now read the **same**
    // lower-cased description, and both spellings of the French abbreviation
    // are accepted — the TS matched on the lower-cased `"deg."` but then ran
    // `/(\d+)\s*deg/` against the *printed* text, so a capitalised "Dég." (or
    // any accented one) silently dealt 0 damage. The KO sweep afterwards is the
    // other half: every other damage source in the engine sweeps, so a card
    // left at 0 PV by a broadside now leaves the board through the normal KO
    // chain (+2 Vol. to its owner, on-KO effects, `activeShip` cleanup).
    if (desc.contains("deg.") || desc.contains("dég.")) && desc.contains("avant") {
        let dmg = number_before(&desc, "deg")
            .or_else(|| number_before(&desc, "dég"))
            .unwrap_or(0);
        if dmg > 0 {
            let opponent_id = player_id.opponent();
            for slot_key in Slot::FRONT {
                let id = state.players.get(opponent_id).board.get(slot_key).cloned();
                if let Some(id) = id {
                    // TS `draft.cards[id].currentPv -= dmg;` — unguarded, so a
                    // board slot holding a dangling id throws and unwinds the
                    // whole `activateShipAbility` (no Volonte spent, no
                    // `usedOnceAbilities` entry, no log).
                    state.get_card_mut(&id)?.current_pv -= dmg;
                }
            }
            sweep_kos(state, registry, ctx, opponent_id, player_id)?;
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
    // Decision §8.37 (`betrayal`): a borrowed body acts for its borrower —
    // `controller()` is `owner` for every card that is not on loan.
    if card.controller() != player_id {
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
        // The TS `getCardDef` lookups live *inside* the sort comparator, and
        // `Array.prototype.sort` never invokes it on a 0- or 1-element slice —
        // so a lone top card whose instance or `defId` is unknown reorders
        // nothing instead of throwing. Keep the lookups behind the same guard.
        if n > 1 {
            let mut costs: Vec<(String, i32)> = Vec::with_capacity(n);
            for id in &top {
                let def_id = state.get_card(id)?.def_id.clone();
                costs.push((id.clone(), registry.get_card_def(&def_id)?.cost));
            }
            // JS `Array.prototype.sort` is stable, and so is `sort_by_key`.
            costs.sort_by_key(|(_, c)| *c);
            top = costs.into_iter().map(|(id, _)| id).collect();
        }
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
        // TS pushes inside `if (target)` …
        if let Some(t) = state.cards.get_mut(target) {
            t.status_effects.push(status(
                StatusEffectType::Trap,
                // permanent until triggered
                -1,
                3,
                instance_id.to_string(),
            ));
        }
        // … then reads `getCardDef(state.cards[targetInstanceId].defId).name`
        // unguarded, so a missing target still throws out of the action.
        let target_name = registry
            .get_card_def(&state.get_card(target)?.def_id)?
            .name
            .clone();
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
        // Decision §8.5: the single heal path.
        heal_unit(state, registry, target, heal)?;
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
///
/// Decision §8.43: `player_id` is the **defender** for Observation while a
/// pending attack is open (the dispatcher routes it), and the acting player for
/// King and Armament; `use_observation_haki` re-checks it either way.
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
    fn debuff_all_enemies_only_reads_effective_def_behind_the_immobilize_guard() {
        use crate::types::{CardDef, CardType, EventEffect, Faction, Rarity, Slot};

        // TS puts `getEffectiveDef(next, slot)` *inside*
        // `if (effect.immobilizeMaxDef !== undefined)`, so an effect without
        // that field never touches the enemy's definitions and cannot fail on
        // them.
        let p = PlayerId::Player1;
        let opp = p.opponent();
        let mut reg = card_registry();
        for (id, max_def) in [("T-DEBUFF", None), ("T-DEBUFF-IMM", Some(9))] {
            let mut ev = CardDef::new(
                id,
                "Intimidation",
                CardType::Event,
                1,
                Faction::Pirate,
                Rarity::C,
                "TEST",
            );
            ev.event_effect = Some(EventEffect::DebuffAllEnemies {
                atk: 2,
                immobilize_max_def: max_def,
            });
            reg.register_card(ev);
        }

        let mut ctx = EngineContext::seeded(9);
        let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
        state.current_player = p;
        state.phase = crate::types::Phase::Main;
        state.players.get_mut(p).volonte = 5;

        // An enemy carrying an object whose definition is not registered:
        // `getEffectiveDef` throws on it, `getCardDef(c.defId)` does not.
        let obj = ctx.generate_instance_id("T-GHOSTOBJ");
        let mut obj_inst = CardInstance::new(obj.clone(), "T-GHOSTOBJ".into(), opp, 0);
        obj_inst.zone = Zone::Board;
        state.add_instance(obj_inst);
        let enemy = ctx.generate_instance_id("MR-001");
        let mut inst = CardInstance::new(enemy.clone(), "MR-001".into(), opp, 3);
        inst.zone = Zone::Board;
        inst.slot = Some(Slot::V1);
        inst.attached_objects.push(obj);
        state.add_instance(inst);
        state
            .players
            .get_mut(opp)
            .board
            .set(Slot::V1, Some(enemy.clone()));

        let plain = ctx.generate_instance_id("T-DEBUFF");
        let mut ev = CardInstance::new(plain.clone(), "T-DEBUFF".into(), p, 0);
        ev.zone = Zone::Hand;
        state.add_instance(ev);
        state.players.get_mut(p).hand.push(plain.clone());

        let immobilizing = ctx.generate_instance_id("T-DEBUFF-IMM");
        let mut ev = CardInstance::new(immobilizing.clone(), "T-DEBUFF-IMM".into(), p, 0);
        ev.zone = Zone::Hand;
        state.add_instance(ev);
        state.players.get_mut(p).hand.push(immobilizing.clone());

        // The variant that *does* carry `immobilizeMaxDef` still throws.
        let err = play_event(&mut state, &reg, &mut ctx, p, &immobilizing).unwrap_err();
        assert_eq!(err.to_string(), "Card not found: T-GHOSTOBJ");

        // The plain debuff goes through and lands its ATK modifier.
        play_event(&mut state, &reg, &mut ctx, p, &plain).unwrap();
        let hit = state.card(&enemy).unwrap();
        assert!(hit.modifiers.iter().any(|m| m.amount == -2));
        assert!(!hit.has_status(StatusEffectType::Immobilize));
    }

    #[test]
    fn tutor_throws_on_a_deck_id_with_no_instance() {
        // TS `getCardDef(draft.cards[id].defId)` inside `findIndex` — a deck
        // entry with no instance throws a TypeError out of the whole
        // `playEvent`, so the cost is refunded and the card stays in hand.
        let p = PlayerId::Player1;
        let (mut state, reg, mut ctx) = game_with_in_hand("BW-027", p);
        state.players.get_mut(p).volonte = 5;
        let instance_id = state.players.get(p).hand.last().unwrap().clone();
        state.players.get_mut(p).deck.insert(0, "nowhere".into());
        let before = state.clone();

        let err = play_event(&mut state, &reg, &mut ctx, p, &instance_id).unwrap_err();
        assert_eq!(err.to_string(), "Instance not found: nowhere");
        assert_eq!(state, before);
    }

    #[test]
    fn rush_buff_writes_the_minus_one_deployed_turn_sentinel() {
        use crate::types::Slot;

        // MG-028 "Coup de Burst" — TS `c.deployedTurn = -1`.
        let p = PlayerId::Player1;
        let (mut state, reg, mut ctx) = game_with_in_hand("MG-028", p);
        state.players.get_mut(p).volonte = 5;
        let instance_id = state.players.get(p).hand.last().unwrap().clone();

        let ally = ctx.generate_instance_id("MG-001");
        let mut inst = CardInstance::new(ally.clone(), "MG-001".into(), p, 3);
        inst.zone = Zone::Board;
        inst.slot = Some(Slot::V1);
        inst.deployed_turn = Some(i64::from(state.turn_number));
        state.add_instance(inst);
        state
            .players
            .get_mut(p)
            .board
            .set(Slot::V1, Some(ally.clone()));

        play_event(&mut state, &reg, &mut ctx, p, &instance_id).unwrap();

        assert_eq!(state.card(&ally).unwrap().deployed_turn, Some(-1));
        // …and the sentinel survives serialisation as the TS engine writes it.
        let json = serde_json::to_value(state.card(&ally).unwrap()).unwrap();
        assert_eq!(json["deployedTurn"], serde_json::json!(-1));
    }

    #[test]
    fn a_ship_front_line_hit_aborts_on_a_slot_with_no_instance() {
        use crate::types::{CardDef, CardType, Faction, Rarity, ShipActive, Slot};

        // TS `draft.cards[id].currentPv -= dmg;` is unguarded, so the throw
        // unwinds `activateShipAbility`: no Volonte spent, no 1x/game use
        // recorded, no log.
        let p = PlayerId::Player1;
        let mut reg = card_registry();
        let mut ship = CardDef::new(
            "T-SHIP",
            "Canonniere",
            CardType::Ship,
            2,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        ship.ship_active = Some(ShipActive {
            name: "Salve".into(),
            cost: 1,
            description: "2 deg. a toute la Ligne Avant ennemie.".into(),
            once_per_game: Some(true),
        });
        reg.register_card(ship);

        let mut ctx = EngineContext::seeded(5);
        let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
        state.current_player = p;
        state.phase = crate::types::Phase::Main;
        state.players.get_mut(p).volonte = 5;

        let sid = ctx.generate_instance_id("T-SHIP");
        let mut inst = CardInstance::new(sid.clone(), "T-SHIP".into(), p, 0);
        inst.zone = Zone::Board;
        state.add_instance(inst);
        state.players.get_mut(p).active_ship = Some(sid.clone());
        // A front slot pointing at an id that has no instance behind it.
        state
            .players
            .get_mut(p.opponent())
            .board
            .set(Slot::V2, Some("nowhere".into()));
        let before = state.clone();

        let err = activate_ship_ability(&mut state, &reg, &mut ctx, p, &sid).unwrap_err();
        assert_eq!(err.to_string(), "Instance not found: nowhere");
        assert_eq!(state, before);
        assert_eq!(state.players.get(p).volonte, 5);
        assert!(state.card(&sid).unwrap().used_once_abilities.is_empty());
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
    fn coordinated_fire_never_reads_the_def_of_a_lone_enemy() {
        use crate::types::Slot;

        // TS `enemies.reduce((a, b) => ...)` with no seed: a 1-element array is
        // returned without the callback ever running, so `getEffectiveDef` is
        // never called and an unregistered lone enemy still takes the damage.
        let p = PlayerId::Player1;
        let (mut state, reg, mut ctx) = game_with_in_hand("MR-024", p);
        state.players.get_mut(p).volonte = 5;
        let instance_id = state.players.get(p).hand.last().unwrap().clone();

        // One marine of our own on the board — this def *is* registered.
        let marine = ctx.generate_instance_id("MR-001");
        let mut inst = CardInstance::new(marine.clone(), "MR-001".into(), p, 3);
        inst.zone = Zone::Board;
        inst.slot = Some(Slot::V1);
        state.add_instance(inst);
        state.players.get_mut(p).board.set(Slot::V1, Some(marine));

        // The single enemy carries a `defId` the registry does not know.
        let ghost = ctx.generate_instance_id("T-GHOST");
        let mut inst = CardInstance::new(ghost.clone(), "T-GHOST".into(), p.opponent(), 9);
        inst.zone = Zone::Board;
        inst.slot = Some(Slot::V1);
        state.add_instance(inst);
        state
            .players
            .get_mut(p.opponent())
            .board
            .set(Slot::V1, Some(ghost.clone()));

        play_event(&mut state, &reg, &mut ctx, p, &instance_id).unwrap();

        assert_eq!(state.card(&ghost).unwrap().current_pv, 8);
        assert!(
            state
                .log
                .iter()
                .any(|e| e.message == "Ordre de Tir : 1 dégâts coordonnés.")
        );
    }

    #[test]
    fn a_one_card_scry_never_reads_the_top_cards_def() {
        use crate::types::Slot;

        // TS sorts the top N inside `Array.prototype.sort`, whose comparator is
        // never invoked for a 1-element slice — so the `getCardDef` lookups
        // cannot throw and the support action still spends the turn.
        let p = PlayerId::Player1;
        let reg = card_registry();
        let mut ctx = EngineContext::seeded(7);
        let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
        state.current_player = p;
        state.phase = crate::types::Phase::Main;

        let nami = ctx.generate_instance_id("MG-003");
        let mut inst = CardInstance::new(nami.clone(), "MG-003".into(), p, 4);
        inst.zone = Zone::Board;
        inst.slot = Some(Slot::A1);
        inst.deployed_turn = Some(0);
        state.add_instance(inst);
        state
            .players
            .get_mut(p)
            .board
            .set(Slot::A1, Some(nami.clone()));
        // A deck of exactly one entry, pointing at an id with no instance.
        state.players.get_mut(p).deck = vec!["nowhere".into()];

        execute_support_action(&mut state, &reg, &mut ctx, p, &nami, None).unwrap();

        assert_eq!(state.players.get(p).deck, vec!["nowhere".to_string()]);
        assert!(state.card(&nami).unwrap().tapped);
        assert!(state.card(&nami).unwrap().used_base_action);
        assert!(
            state
                .log
                .iter()
                .any(|e| e.message == "Nami utilise Prévisions : réorganise le dessus du deck.")
        );
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

    /// A character of `def_id` placed straight onto `owner`'s board in `slot`,
    /// deployed long enough ago to have no summoning sickness.
    fn put_on_board(
        state: &mut GameState,
        registry: &CardRegistry,
        ctx: &mut EngineContext,
        def_id: &str,
        owner: PlayerId,
        slot: Slot,
    ) -> String {
        let pv = registry
            .get_card_def(def_id)
            .ok()
            .and_then(|d| d.pv)
            .unwrap_or(1);
        let id = ctx.generate_instance_id(def_id);
        let mut inst = CardInstance::new(id.clone(), def_id.to_string(), owner, pv);
        inst.zone = Zone::Board;
        inst.slot = Some(slot);
        inst.deployed_turn = Some(0);
        state.add_instance(inst);
        state
            .players
            .get_mut(owner)
            .board
            .set(slot, Some(id.clone()));
        id
    }

    // --------------------------------------------------------
    // §8.11 — ship active, front-line broadside
    // --------------------------------------------------------

    #[test]
    fn a_capitalised_front_line_broadside_deals_its_damage_and_sweeps_the_kos() {
        use crate::types::{CardDef, CardType, Faction, Rarity, ShipActive};

        // Decision §8.11. Before the fix the branch tested the *lower-cased*
        // description for "deg." (so an accented "Dég." never matched) and
        // extracted from the *printed* one, and nothing swept the KOs.
        let p = PlayerId::Player1;
        let opp = p.opponent();
        let mut reg = card_registry();
        let mut ship = CardDef::new(
            "T-SHIP-ACCENT",
            "Canonniere",
            CardType::Ship,
            2,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        ship.ship_active = Some(ShipActive {
            name: "Salve".into(),
            cost: 1,
            description: "3 Dég. à toute la Ligne Avant ennemie.".into(),
            once_per_game: Some(true),
        });
        reg.register_card(ship);

        let mut ctx = EngineContext::seeded(5);
        let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
        state.current_player = p;
        state.phase = crate::types::Phase::Main;
        state.players.get_mut(p).volonte = 5;

        let sid = ctx.generate_instance_id("T-SHIP-ACCENT");
        let mut inst = CardInstance::new(sid.clone(), "T-SHIP-ACCENT".into(), p, 0);
        inst.zone = Zone::Board;
        state.add_instance(inst);
        state.players.get_mut(p).active_ship = Some(sid.clone());

        // A 3-PV front-row enemy and a back-row one that must not be touched.
        let front = put_on_board(&mut state, &reg, &mut ctx, "MR-001", opp, Slot::V1);
        state.card_mut(&front).unwrap().current_pv = 3;
        let back = put_on_board(&mut state, &reg, &mut ctx, "MR-002", opp, Slot::A1);
        let vol_before = state.players.get(opp).volonte;

        activate_ship_ability(&mut state, &reg, &mut ctx, p, &sid).unwrap();

        // 3 damage, not 0 — and the 0-PV body left through the KO chain.
        assert_eq!(state.card(&front).unwrap().zone, Zone::Graveyard);
        assert!(state.players.get(opp).board.get(Slot::V1).is_none());
        assert!(state.players.get(opp).graveyard.contains(&front));
        assert_eq!(state.players.get(opp).volonte, vol_before + 2);
        assert!(state.log.iter().any(|e| e.message == "Coby est KO !"));
        // The back row is untouched, and the activation still logs.
        assert_eq!(state.card(&back).unwrap().current_pv, 4);
        assert!(
            state
                .log
                .iter()
                .any(|e| e.message == "Canonniere active Salve !")
        );
    }

    #[test]
    fn the_unaccented_broadside_keeps_its_exact_old_reading() {
        use crate::types::{CardDef, CardType, Faction, Rarity, ShipActive};

        // Decision §8.54(b): the branch that keeps parsing free text parses it
        // exactly as before — same substring tests, same `/(\d+)\s*deg/`.
        let p = PlayerId::Player1;
        let opp = p.opponent();
        let mut reg = card_registry();
        let mut ship = CardDef::new(
            "T-SHIP-PLAIN",
            "Canonniere",
            CardType::Ship,
            2,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        ship.ship_active = Some(ShipActive {
            name: "Salve".into(),
            cost: 1,
            description: "2 deg. a toute la Ligne Avant ennemie.".into(),
            once_per_game: Some(true),
        });
        reg.register_card(ship);

        let mut ctx = EngineContext::seeded(6);
        let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
        state.current_player = p;
        state.phase = crate::types::Phase::Main;
        state.players.get_mut(p).volonte = 5;

        let sid = ctx.generate_instance_id("T-SHIP-PLAIN");
        let mut inst = CardInstance::new(sid.clone(), "T-SHIP-PLAIN".into(), p, 0);
        inst.zone = Zone::Board;
        state.add_instance(inst);
        state.players.get_mut(p).active_ship = Some(sid.clone());
        let front = put_on_board(&mut state, &reg, &mut ctx, "MR-001", opp, Slot::V2);

        activate_ship_ability(&mut state, &reg, &mut ctx, p, &sid).unwrap();

        assert_eq!(state.card(&front).unwrap().current_pv, 4 - 2);
        assert_eq!(state.card(&front).unwrap().zone, Zone::Board);
    }

    // --------------------------------------------------------
    // §8.36 — EventEffect gaps
    // --------------------------------------------------------

    /// Registers `def_id` as a 0-cost event carrying `effect`.
    fn register_event(reg: &mut CardRegistry, def_id: &str, name: &str, effect: EventEffect) {
        use crate::types::{CardDef, CardType, Faction, Rarity};
        let mut ev = CardDef::new(
            def_id,
            name,
            CardType::Event,
            0,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        ev.event_effect = Some(effect);
        reg.register_card(ev);
    }

    #[test]
    fn damage_enemies_single_hits_exactly_one_enemy() {
        // Decision §8.36: `single` is not `all` — the TS looped over the whole
        // enemy board for every target mode.
        let p = PlayerId::Player1;
        let opp = p.opponent();
        let mut reg = card_registry();
        register_event(
            &mut reg,
            "T-SNIPE",
            "Tir Isole",
            EventEffect::DamageEnemies {
                amount: 3,
                target: DamageTarget::Single,
                cursed_bonus: None,
                sand: None,
                destroy_ships: None,
            },
        );

        let mut ctx = EngineContext::seeded(12);
        let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
        state.current_player = p;
        state.phase = crate::types::Phase::Main;
        state.players.get_mut(p).volonte = 5;
        let ev = ctx.generate_instance_id("T-SNIPE");
        let mut inst = CardInstance::new(ev.clone(), "T-SNIPE".into(), p, 0);
        inst.zone = Zone::Hand;
        state.add_instance(inst);
        state.players.get_mut(p).hand.push(ev.clone());

        // Front row: Coby (ATK 2) and Helmeppo (ATK 3) — the stronger one is
        // hit. The back-row Tashigi (ATK 4) is outside the front pool.
        let weak = put_on_board(&mut state, &reg, &mut ctx, "MR-001", opp, Slot::V1);
        let strong = put_on_board(&mut state, &reg, &mut ctx, "MR-002", opp, Slot::V2);
        let back = put_on_board(&mut state, &reg, &mut ctx, "MR-003", opp, Slot::A1);

        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();

        assert_eq!(state.card(&strong).unwrap().current_pv, 4 - 3);
        assert_eq!(state.card(&weak).unwrap().current_pv, 4);
        assert_eq!(state.card(&back).unwrap().current_pv, 5);
    }

    #[test]
    fn damage_enemies_single_falls_back_to_the_captain_on_an_empty_board() {
        let p = PlayerId::Player1;
        let opp = p.opponent();
        let mut reg = card_registry();
        register_event(
            &mut reg,
            "T-SNIPE2",
            "Tir Isole",
            EventEffect::DamageEnemies {
                amount: 2,
                target: DamageTarget::Single,
                cursed_bonus: None,
                sand: None,
                destroy_ships: None,
            },
        );
        let mut ctx = EngineContext::seeded(13);
        let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
        state.current_player = p;
        state.phase = crate::types::Phase::Main;
        state.players.get_mut(p).volonte = 5;
        let ev = ctx.generate_instance_id("T-SNIPE2");
        let mut inst = CardInstance::new(ev.clone(), "T-SNIPE2".into(), p, 0);
        inst.zone = Zone::Hand;
        state.add_instance(inst);
        state.players.get_mut(p).hand.push(ev.clone());

        let pv_before = state.players.get(opp).captain.current_pv;
        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();
        assert_eq!(state.players.get(opp).captain.current_pv, pv_before - 2);
    }

    #[test]
    fn heal_ally_without_all_allies_heals_the_most_wounded_one() {
        use crate::types::StatusEffectType;

        // Decision §8.36: the TS `healAlly` single branch was dead code.
        let p = PlayerId::Player1;
        let mut reg = card_registry();
        register_event(
            &mut reg,
            "T-HEAL1",
            "Soin",
            EventEffect::HealAlly {
                amount: 2,
                all_allies: None,
            },
        );
        let mut ctx = EngineContext::seeded(14);
        let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
        state.current_player = p;
        state.phase = crate::types::Phase::Main;
        state.players.get_mut(p).volonte = 5;
        let ev = ctx.generate_instance_id("T-HEAL1");
        let mut inst = CardInstance::new(ev.clone(), "T-HEAL1".into(), p, 0);
        inst.zone = Zone::Hand;
        state.add_instance(inst);
        state.players.get_mut(p).hand.push(ev.clone());

        let scratched = put_on_board(&mut state, &reg, &mut ctx, "MR-001", p, Slot::V1);
        state.card_mut(&scratched).unwrap().current_pv = 3;
        let wounded = put_on_board(&mut state, &reg, &mut ctx, "MR-002", p, Slot::V2);
        state.card_mut(&wounded).unwrap().current_pv = 1;

        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();

        assert_eq!(state.card(&wounded).unwrap().current_pv, 3);
        assert_eq!(state.card(&scratched).unwrap().current_pv, 3);
        // …and the heal still honours `noHeal` (decision §8.5).
        assert!(
            !state
                .card(&wounded)
                .unwrap()
                .has_status(StatusEffectType::NoHeal)
        );
    }

    #[test]
    fn justice_absolue_buffs_marines_only() {
        // Decision §8.36: `MR-022` carries `filter: { faction: "marine" }`,
        // which the TS `buffAllies` arm never read.
        let p = PlayerId::Player1;
        let (mut state, reg, mut ctx) = game_with_in_hand("MR-022", p);
        state.players.get_mut(p).volonte = 5;
        let ev = state.players.get(p).hand.last().unwrap().clone();

        let marine = put_on_board(&mut state, &reg, &mut ctx, "MR-001", p, Slot::V1);
        let pirate = put_on_board(&mut state, &reg, &mut ctx, "MG-001", p, Slot::V2);

        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();

        assert!(
            state
                .card(&marine)
                .unwrap()
                .modifiers
                .iter()
                .any(|m| m.source == "Justice Absolue" && m.amount == 2)
        );
        assert!(
            !state
                .card(&pirate)
                .unwrap()
                .modifiers
                .iter()
                .any(|m| m.source == "Justice Absolue")
        );
    }

    #[test]
    fn tempete_de_sable_lowers_the_max_pv_instead_of_dealing_one_more_damage() {
        use crate::board::{heal_unit, max_pv_of};

        // Decision §8.36: "Sable" is a permanent PV loss, which the port
        // approximated with `dmg += 1`.
        let p = PlayerId::Player1;
        let opp = p.opponent();
        let (mut state, reg, mut ctx) = game_with_in_hand("BW-023", p);
        state.players.get_mut(p).volonte = 5;
        let ev = state.players.get(p).hand.last().unwrap().clone();
        let victim = put_on_board(&mut state, &reg, &mut ctx, "MR-001", opp, Slot::V1);

        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();

        // 2 damage — not 3 — plus one point of maximum PV, for good.
        assert_eq!(state.card(&victim).unwrap().current_pv, 4 - 2);
        assert_eq!(state.card(&victim).unwrap().pv_max_loss, Some(1));
        assert_eq!(max_pv_of(&state, &reg, &victim).unwrap(), Some(3));

        // A later full heal cannot climb back to the printed 4.
        heal_unit(&mut state, &reg, &victim, 9).unwrap();
        assert_eq!(state.card(&victim).unwrap().current_pv, 3);
    }

    #[test]
    fn a_sand_event_never_leaves_a_unit_above_its_new_maximum() {
        use crate::board::max_pv_of;

        // The three permanent-max-PV-loss paths (this one, `resolve_attack`'s
        // `permanentPvLoss` and the Sand arm of `apply_element_to_captain`)
        // agree: the current PV follows the maximum down. A unit whose damage
        // is absorbed — here Chopper's `damageReduction` passive brings the
        // 2-point hit to 1 — must not be left standing above its own maximum.
        let p = PlayerId::Player1;
        let opp = p.opponent();
        let (mut state, reg, mut ctx) = game_with_in_hand("BW-023", p);
        state.players.get_mut(p).volonte = 5;
        let ev = state.players.get(p).hand.last().unwrap().clone();
        let victim = put_on_board(&mut state, &reg, &mut ctx, "MR-001", opp, Slot::V1);
        // Heal-proof setup: a unit at full PV whose damage is undone before we
        // look, i.e. the maximum is what constrains it.
        state.card_mut(&victim).unwrap().current_pv = 4;

        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();
        // 4 - 2 damage = 2, under the new maximum of 3, so nothing to clamp.
        assert_eq!(max_pv_of(&state, &reg, &victim).unwrap(), Some(3));
        assert_eq!(state.card(&victim).unwrap().current_pv, 2);

        // Now the same sand on a unit the damage cannot reach: the max drops
        // to 2 and the current PV is pulled down with it.
        state.card_mut(&victim).unwrap().current_pv = 3;
        sand_only(&mut state, &reg, &victim).unwrap();
        assert_eq!(max_pv_of(&state, &reg, &victim).unwrap(), Some(2));
        assert_eq!(
            state.card(&victim).unwrap().current_pv,
            2,
            "the current PV never stays above the new maximum"
        );
    }

    /// `damage_enemy` with a `sand` flag and no damage at all — the isolated
    /// max-PV-loss half of `BW-023`.
    fn sand_only(
        state: &mut GameState,
        registry: &CardRegistry,
        instance_id: &str,
    ) -> Result<(), EngineError> {
        super::damage_enemy(state, registry, instance_id, 0, None, Some(true))
    }

    // --------------------------------------------------------
    // §8.37 — custom event ids
    // --------------------------------------------------------

    #[test]
    fn embargo_blocks_equipping_and_ship_deploys_for_exactly_one_opponent_turn() {
        use crate::actions::get_valid_actions;

        let p = PlayerId::Player1;
        let opp = p.opponent();
        let (mut state, reg, mut ctx) = game_with_in_hand("MR-027", p);
        state.players.get_mut(p).volonte = 5;
        let ev = state.players.get(p).hand.last().unwrap().clone();

        // A ship in the opponent's hand, affordable on their turn.
        let ship = ctx.generate_instance_id("MR-018");
        let mut inst = CardInstance::new(ship.clone(), "MR-018".into(), opp, 0);
        inst.zone = Zone::Hand;
        state.add_instance(inst);
        state.players.get_mut(opp).hand.push(ship.clone());

        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();
        assert_eq!(state.players.get(opp).embargo_turns, Some(1));

        // Their turn: neither action is executable, nor offered.
        state.current_player = opp;
        state.phase = crate::types::Phase::Main;
        state.players.get_mut(opp).volonte = 5;
        let deploy = GameAction::DeployShip {
            instance_id: ship.clone(),
        };
        let equip = GameAction::EquipObject {
            object_instance_id: "whatever".into(),
            target_instance_id: "whoever".into(),
            target_is_captain: None,
        };
        assert_eq!(
            execute_action(&mut state, &reg, &mut ctx, &deploy)
                .unwrap_err()
                .to_string(),
            EMBARGO_SHIP
        );
        assert_eq!(
            execute_action(&mut state, &reg, &mut ctx, &equip)
                .unwrap_err()
                .to_string(),
            EMBARGO_EQUIP
        );
        let offered = get_valid_actions(&state, &reg, opp).unwrap();
        assert!(
            !offered
                .iter()
                .any(|a| matches!(a, GameAction::DeployShip { .. }))
        );
        assert!(
            !offered
                .iter()
                .any(|a| matches!(a, GameAction::EquipObject { .. }))
        );
        // Everything else still works — the ban is exactly two actions wide.
        assert!(offered.iter().any(|a| matches!(a, GameAction::EndTurn)));

        // …and it lapses at the end of that same turn.
        state.end_turn(&reg, &EngineContext::seeded(1)).unwrap();
        assert_eq!(state.players.get(opp).embargo_turns, None);
        state.current_player = opp;
        state.phase = crate::types::Phase::Main;
        state.players.get_mut(opp).volonte = 5;
        execute_action(&mut state, &reg, &mut ctx, &deploy).unwrap();
        assert_eq!(state.players.get(opp).active_ship, Some(ship));
    }

    #[test]
    fn pluie_artificielle_blocks_enemy_heals_for_two_of_their_turns() {
        use crate::board::heal_unit;
        use crate::types::StatusEffectType;

        let p = PlayerId::Player1;
        let opp = p.opponent();
        let (mut state, reg, mut ctx) = game_with_in_hand("BW-028", p);
        state.players.get_mut(p).volonte = 5;
        let ev = state.players.get(p).hand.last().unwrap().clone();
        let enemy = put_on_board(&mut state, &reg, &mut ctx, "MR-001", opp, Slot::V1);
        state.card_mut(&enemy).unwrap().current_pv = 1;

        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();
        assert!(
            state
                .card(&enemy)
                .unwrap()
                .has_status(StatusEffectType::NoHeal)
        );

        // Still the caster's turn: already dry.
        heal_unit(&mut state, &reg, &enemy, 3).unwrap();
        assert_eq!(state.card(&enemy).unwrap().current_pv, 1);

        // Their turn under the rain: the status ticks 2 -> 1 and still blocks,
        // exactly like every other two-turn status in this engine (a status
        // only counts down on its bearer's own turns).
        state.current_player = opp;
        state.process_start_of_turn_effects();
        assert_eq!(
            state
                .card(&enemy)
                .unwrap()
                .status(StatusEffectType::NoHeal)
                .unwrap()
                .turns_remaining,
            1
        );
        heal_unit(&mut state, &reg, &enemy, 3).unwrap();
        assert_eq!(state.card(&enemy).unwrap().current_pv, 1);

        // Then it lapses at the start of the turn after.
        state.process_start_of_turn_effects();
        assert!(
            !state
                .card(&enemy)
                .unwrap()
                .has_status(StatusEffectType::NoHeal)
        );
        heal_unit(&mut state, &reg, &enemy, 3).unwrap();
        assert_eq!(state.card(&enemy).unwrap().current_pv, 4);
    }

    #[test]
    fn a_borrowed_body_is_only_offered_its_former_allies_as_targets() {
        // Decision §8.37 at the *targeting* layer: `get_valid_targets` reads
        // the attacker's `controller()`, so the enemy side of a borrowed body
        // is the borrower's opponent — its own former camp. Reading `owner`
        // inverted it: the loan was offered the borrower's units and itself,
        // and the crew it was lent against was invisible.
        use crate::board::get_valid_targets;

        let p = PlayerId::Player1;
        let opp = p.opponent();
        let (mut state, reg, mut ctx) = game_with_in_hand("BW-024", p);
        state.players.get_mut(p).volonte = 5;
        let ev = state.players.get(p).hand.last().unwrap().clone();

        let helmeppo = put_on_board(&mut state, &reg, &mut ctx, "MR-002", opp, Slot::V1);
        let tashigi = put_on_board(&mut state, &reg, &mut ctx, "MR-003", opp, Slot::V2);
        let ally = put_on_board(&mut state, &reg, &mut ctx, "MG-002", p, Slot::V2);

        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();
        assert_eq!(state.card(&helmeppo).unwrap().controlled_by, Some(p));

        let targets = get_valid_targets(&state, &reg, &helmeppo, false).unwrap();
        assert!(
            targets.character_targets.contains(&tashigi),
            "the borrowed body must be able to turn on its former ally: {targets:?}"
        );
        assert!(
            !targets.character_targets.contains(&ally),
            "the borrower's own units are not targets: {targets:?}"
        );
        assert!(
            !targets.character_targets.contains(&helmeppo),
            "and neither is the borrowed body itself: {targets:?}"
        );
        assert!(
            !targets.can_target_captain,
            "the enemy crew is still standing"
        );

        // The enumerator follows the same rule, so no friendly fire is offered.
        let actions = crate::actions::get_valid_actions(&state, &reg, p).unwrap();
        assert!(
            actions.iter().any(|a| matches!(
                a,
                GameAction::BaseAttack { attacker_instance_id, target_instance_id, .. }
                    if attacker_instance_id == &helmeppo && target_instance_id == &tashigi
            )),
            "the loan should be offered an attack on its former ally"
        );
        assert!(
            !actions.iter().any(|a| matches!(
                a,
                GameAction::BaseAttack { attacker_instance_id, target_instance_id, .. }
                    if attacker_instance_id == &helmeppo
                        && (target_instance_id == &ally || target_instance_id == &helmeppo)
            )),
            "no friendly fire from a borrowed body"
        );
        // An object is a permanent attachment, so the loan is never an equip
        // target either (decision §8.57 x §8.37 — `equip_object` refuses it).
        assert!(
            !actions.iter().any(|a| matches!(
                a,
                GameAction::EquipObject { target_instance_id, .. }
                    if target_instance_id == &helmeppo
            )),
            "a borrowed body cannot be equipped"
        );
    }

    #[test]
    fn a_loan_returns_beside_a_body_pushed_into_its_home_slot() {
        // Decision §8.37 assumed `loan_return_slot` was still free at the end
        // of the turn. Decision §8.38 then made `Impact` imply a pushback, so
        // the borrower's own attacks can shove an enemy from V* into the A*
        // cell the loan vacated. The returning body must not overwrite that
        // occupant (which would leave it in no board cell at all).
        let p = PlayerId::Player1;
        let opp = p.opponent();
        let (mut state, reg, mut ctx) = game_with_in_hand("BW-024", p);
        state.players.get_mut(p).volonte = 5;
        let ev = state.players.get(p).hand.last().unwrap().clone();

        // Helmeppo (cost 2, ATK 3) is the pick; it sits in A1, so A1 is the
        // slot the loan will try to return to.
        let helmeppo = put_on_board(&mut state, &reg, &mut ctx, "MR-002", opp, Slot::A1);
        let coby = put_on_board(&mut state, &reg, &mut ctx, "MR-001", opp, Slot::V1);
        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();
        assert_eq!(
            state.card(&helmeppo).unwrap().loan_return_slot,
            Some(Slot::A1)
        );

        // Push Coby out of V1 into the now-empty A1 by hand — the pushback an
        // `Impact` attack performs.
        {
            let board = &mut state.players.get_mut(opp).board;
            board.set(Slot::V1, None);
            board.set(Slot::A1, Some(coby.clone()));
        }
        state.card_mut(&coby).unwrap().slot = Some(Slot::A1);

        state.end_turn(&reg, &EngineContext::seeded(1)).unwrap();

        // Coby keeps A1, the loan lands in a free slot of its own camp, and
        // every board cell still agrees with the instance it holds.
        assert_eq!(state.players.get(opp).board.get(Slot::A1), Some(&coby));
        assert_eq!(state.card(&coby).unwrap().slot, Some(Slot::A1));
        let back = state.card(&helmeppo).unwrap();
        assert_eq!(back.controlled_by, None);
        assert_eq!(back.loan_return_slot, None);
        assert_eq!(back.zone, Zone::Board);
        let home = back.slot.expect("it is back on its owner's board");
        assert_ne!(home, Slot::A1);
        assert_eq!(state.players.get(opp).board.get(home), Some(&helmeppo));
        for slot in Slot::ALL {
            if let Some(id) = state.players.get(opp).board.get(slot) {
                assert_eq!(
                    state.card(id).unwrap().slot,
                    Some(slot),
                    "board cell {slot:?} and instance disagree"
                );
            }
        }
    }

    /// Decision §8.37 follow-up (b) — when the owner's board has no free cell
    /// at all, the homeless loan "leaves the board through `remove_from_board`
    /// instead of squatting in the borrower's camp", and that is *all* it does:
    /// a failed hand-back is not a KO, so no +2 Volonté moves, no on-KO trigger
    /// fires and no log line is written for it.
    #[test]
    fn a_homeless_loan_is_removed_without_being_treated_as_a_ko() {
        let p = PlayerId::Player1;
        let opp = p.opponent();
        let (mut state, reg, mut ctx) = game_with_in_hand("BW-024", p);
        state.players.get_mut(p).volonte = 5;
        let ev = state.players.get(p).hand.last().unwrap().clone();

        let helmeppo = put_on_board(&mut state, &reg, &mut ctx, "MR-002", opp, Slot::A1);
        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();
        assert_eq!(state.card(&helmeppo).unwrap().controlled_by, Some(p));

        // Refill every cell of the owner's board while the body is away — no
        // shipped effect can do this, which is why the arm is unreachable in a
        // real game; it is still a total fallback and must not invent a KO.
        for slot in Slot::ALL {
            let filler = put_on_board(&mut state, &reg, &mut ctx, "MR-001", opp, slot);
            assert_eq!(state.players.get(opp).board.get(slot), Some(&filler));
        }
        let vol_before = state.players.get(opp).volonte;
        let ko_flag_before = state.players.get(opp).char_ko_ed_this_game;
        let logs_before = state.log.len();

        state.end_turn(&reg, &EngineContext::seeded(1)).unwrap();

        let gone = state.card(&helmeppo).unwrap();
        assert_ne!(gone.zone, Zone::Board);
        assert_eq!(gone.slot, None);
        assert_eq!(gone.controlled_by, None);
        assert_eq!(gone.loan_return_slot, None);
        // No KO bonus and no on-KO bookkeeping: this is a removal, not a kill.
        assert_eq!(state.players.get(opp).volonte, vol_before);
        assert_eq!(state.players.get(opp).char_ko_ed_this_game, ko_flag_before);
        // …and no log line about it (the only new lines are the turn's own).
        assert!(
            !state.log[logs_before..]
                .iter()
                .any(|l| l.message.contains("n'a plus de place")),
            "the fallback writes no log line of its own"
        );
    }

    #[test]
    fn trahison_borrows_a_cheap_enemy_for_the_turn_and_gives_it_back() {
        use crate::combat::get_attacker_owner;

        let p = PlayerId::Player1;
        let opp = p.opponent();
        let (mut state, reg, mut ctx) = game_with_in_hand("BW-024", p);
        state.players.get_mut(p).volonte = 5;
        let ev = state.players.get(p).hand.last().unwrap().clone();

        // Coby (cost 2, ATK 2), Helmeppo (cost 2, ATK 3) and Tashigi
        // (cost 3, ATK 4): the strongest of the *affordable* two is taken.
        let coby = put_on_board(&mut state, &reg, &mut ctx, "MR-001", opp, Slot::V1);
        let helmeppo = put_on_board(&mut state, &reg, &mut ctx, "MR-002", opp, Slot::V2);
        let tashigi = put_on_board(&mut state, &reg, &mut ctx, "MR-003", opp, Slot::V3);
        state.card_mut(&coby).unwrap().current_pv = 2;
        state.card_mut(&helmeppo).unwrap().tapped = true;

        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();

        let borrowed = state.card(&helmeppo).unwrap();
        assert_eq!(borrowed.controlled_by, Some(p));
        assert_eq!(borrowed.loan_return_slot, Some(Slot::V2));
        assert_eq!(borrowed.owner, opp, "ownership never moves");
        assert!(!borrowed.tapped);
        let lent_slot = borrowed.slot.expect("on the borrower's board");
        assert_eq!(
            state.players.get(p).board.get(lent_slot),
            Some(&helmeppo.clone())
        );
        assert!(state.players.get(opp).board.get(Slot::V2).is_none());
        assert_eq!(state.card(&tashigi).unwrap().controlled_by, None);

        // It fights for its borrower: the target side is the borrower's
        // opponent, i.e. its own former allies.
        assert_eq!(get_attacker_owner(&state, &helmeppo).unwrap(), p);
        let vol_before = state.players.get(opp).volonte;
        execute_action(
            &mut state,
            &reg,
            &mut ctx,
            &GameAction::BaseAttack {
                attacker_instance_id: helmeppo.clone(),
                target_instance_id: coby.clone(),
                target_is_captain: None,
            },
        )
        .unwrap();
        execute_action(&mut state, &reg, &mut ctx, &GameAction::PassCounter).unwrap();
        assert_eq!(state.card(&coby).unwrap().zone, Zone::Graveyard);
        // The +2 Vol. for the lost ally goes to its owner, not to the borrower.
        assert_eq!(state.players.get(opp).volonte, vol_before + 2);
        assert!(state.players.get(opp).graveyard.contains(&coby));

        // End of the borrower's turn: home it goes.
        state.end_turn(&reg, &EngineContext::seeded(1)).unwrap();
        let back = state.card(&helmeppo).unwrap();
        assert_eq!(back.controlled_by, None);
        assert_eq!(back.loan_return_slot, None);
        assert_eq!(back.slot, Some(Slot::V2));
        assert_eq!(back.owner, opp);
        assert_eq!(
            state.players.get(opp).board.get(Slot::V2),
            Some(&helmeppo.clone())
        );
        assert!(state.players.get(p).board.get(lent_slot).is_none());
    }

    /// Decision §8.29 — "the equipment travels with its bearer" on *every*
    /// path that moves a bearer between cells, not only the free move: the
    /// betrayal take-over and the loan return both drag the attachments along,
    /// so a serialised object's `slot` always names its bearer's cell.
    #[test]
    fn a_loan_drags_the_bearers_equipment_there_and_back() {
        let p = PlayerId::Player1;
        let opp = p.opponent();
        let (mut state, reg, mut ctx) = game_with_in_hand("BW-024", p);
        state.players.get_mut(p).volonte = 5;
        let ev = state.players.get(p).hand.last().unwrap().clone();

        let helmeppo = put_on_board(&mut state, &reg, &mut ctx, "MR-002", opp, Slot::V2);
        // A weapon (`MR-012`) riding on the body it is attached to.
        let sabre = "MR-012@loan-test".to_string();
        let mut obj = crate::state::CardInstance::new(sabre.clone(), "MR-012".into(), opp, 0);
        obj.zone = Zone::Board;
        obj.slot = Some(Slot::V2);
        state.cards.insert(sabre.clone(), obj);
        state
            .card_mut(&helmeppo)
            .unwrap()
            .attached_objects
            .push(sabre.clone());

        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();
        let lent_slot = state.card(&helmeppo).unwrap().slot.unwrap();
        assert_eq!(
            state.card(&sabre).unwrap().slot,
            Some(lent_slot),
            "the sabre followed its bearer into the borrower's camp"
        );

        state.end_turn(&reg, &EngineContext::seeded(1)).unwrap();
        assert_eq!(state.card(&helmeppo).unwrap().slot, Some(Slot::V2));
        assert_eq!(
            state.card(&sabre).unwrap().slot,
            Some(Slot::V2),
            "and back home again"
        );
    }

    #[test]
    fn an_unknown_custom_id_keeps_its_flavour_log() {
        // Decision §8.37: exactly six ids are legal; nothing is invented.
        let p = PlayerId::Player1;
        let mut reg = card_registry();
        register_event(
            &mut reg,
            "T-CUSTOM",
            "Mystere",
            EventEffect::Custom {
                id: "notAThing".into(),
                description: "Rien du tout.".into(),
            },
        );
        let mut ctx = EngineContext::seeded(15);
        let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
        state.current_player = p;
        state.phase = crate::types::Phase::Main;
        state.players.get_mut(p).volonte = 5;
        let ev = ctx.generate_instance_id("T-CUSTOM");
        let mut inst = CardInstance::new(ev.clone(), "T-CUSTOM".into(), p, 0);
        inst.zone = Zone::Hand;
        state.add_instance(inst);
        state.players.get_mut(p).hand.push(ev.clone());

        let before = state.clone();
        play_event(&mut state, &reg, &mut ctx, p, &ev).unwrap();
        assert!(
            state
                .log
                .iter()
                .any(|e| e.message == "Mystere : Rien du tout.")
        );
        assert_eq!(state.players.get(p).embargo_turns, None);
        assert_eq!(
            state.players.get(p.opponent()).embargo_turns,
            before.players.get(p.opponent()).embargo_turns
        );
    }

    // --------------------------------------------------------
    // §8.43 — Observation Haki is the defender's reaction
    // --------------------------------------------------------

    /// A game on turn 5 (Observation unlocked) with `p` attacking `opp`'s Coby.
    fn game_with_a_pending_attack() -> (GameState, CardRegistry, EngineContext, String) {
        let p = PlayerId::Player1;
        let opp = p.opponent();
        let reg = card_registry();
        let mut ctx = EngineContext::seeded(21);
        let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
        state.current_player = p;
        state.phase = crate::types::Phase::Main;
        state.turn_number = 5;
        let attacker = put_on_board(&mut state, &reg, &mut ctx, "MG-001", p, Slot::V1);
        let target = put_on_board(&mut state, &reg, &mut ctx, "MR-001", opp, Slot::V1);
        crate::combat::declare_base_attack(&mut state, &reg, &ctx, &attacker, &target, false)
            .unwrap();
        assert!(state.pending_attack.is_some());
        (state, reg, ctx, target)
    }

    #[test]
    fn only_the_defender_can_dodge_with_observation_haki() {
        let p = PlayerId::Player1;
        let opp = p.opponent();
        let (mut state, reg, mut ctx, _target) = game_with_a_pending_attack();

        // The attacker asking directly is refused, and burns nothing.
        let err = crate::haki::use_observation_haki(&mut state, p).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Only the defender can use Observation Haki"
        );
        assert!(state.pending_attack.is_some());
        assert!(!state.players.get(p).observation_used);
        assert!(!state.players.get(opp).observation_used);

        // `cannotBeDodged` still wins over the defender's reaction.
        state.pending_attack.as_mut().unwrap().cannot_be_dodged = Some(true);
        let dodge = GameAction::UseHaki {
            haki_type: HakiType::Observation,
            target_instance_id: None,
        };
        assert_eq!(
            execute_action(&mut state, &reg, &mut ctx, &dodge)
                .unwrap_err()
                .to_string(),
            "This attack cannot be dodged"
        );

        // The action is routed to the defender: their flag is the one spent.
        state.pending_attack.as_mut().unwrap().cannot_be_dodged = Some(false);
        execute_action(&mut state, &reg, &mut ctx, &dodge).unwrap();
        assert!(state.pending_attack.is_none());
        assert!(state.players.get(opp).observation_used);
        assert!(!state.players.get(p).observation_used);
    }

    #[test]
    fn king_haki_still_goes_to_the_acting_player() {
        // Decision §8.43 only re-routes Observation.
        let p = PlayerId::Player1;
        let (mut state, reg, mut ctx, _t) = game_with_a_pending_attack();
        state.turn_number = 10;
        execute_action(
            &mut state,
            &reg,
            &mut ctx,
            &GameAction::UseHaki {
                haki_type: HakiType::King,
                target_instance_id: None,
            },
        )
        .unwrap();
        // King Haki was spent by `p` — the acting player — and the defender's
        // Observation flag was never touched.
        assert!(state.players.get(p).king_used);
        assert!(!state.players.get(p.opponent()).king_used);
        assert!(!state.players.get(p.opponent()).observation_used);
    }

    // --------------------------------------------------------
    // §8.54 — free-text parsing policy
    // --------------------------------------------------------

    #[test]
    fn the_migrated_support_specials_resolve_through_fields_not_their_text() {
        use crate::combat::declare_special_attack;
        use crate::types::StatusEffectType;

        // Decision §8.54(a): the three text-only support specials moved to
        // structured fields in the TS data and the Rust catalogue together
        // (§8.38). The data carries them…
        let reg = card_registry();
        for id in ["RH-004", "BW-005"] {
            let spec = reg
                .get_card_def(id)
                .unwrap()
                .special_attack
                .as_ref()
                .expect("support special")
                .clone();
            assert_eq!(spec.taunt, Some(true), "{id} taunts through its field");
        }
        let stimulant = reg
            .get_card_def("RH-009")
            .unwrap()
            .special_attack
            .as_ref()
            .expect("support special")
            .clone();
        assert_eq!(stimulant.buff_ally_atk, Some(2));
        assert_eq!(stimulant.cleanse, Some(true));

        // …and the engine reads the field, not the French sentence: a twin of
        // Rockstar with the very same description but no `taunt` field taunts
        // nobody.
        let p = PlayerId::Player1;
        let opp = p.opponent();
        let mut reg = card_registry();
        let mut twin = reg.get_card_def("RH-004").unwrap().clone();
        twin.id = "T-NOFIELD".into();
        if let Some(spec) = twin.special_attack.as_mut() {
            spec.taunt = None;
        }
        assert!(
            twin.special_attack
                .as_ref()
                .unwrap()
                .description
                .as_deref()
                .unwrap()
                .contains("doit cibler")
        );
        reg.register_card(twin);

        let mut ctx = EngineContext::seeded(31);
        let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
        state.current_player = p;
        state.phase = crate::types::Phase::Main;
        state.players.get_mut(p).volonte = 5;

        let mute = put_on_board(&mut state, &reg, &mut ctx, "T-NOFIELD", p, Slot::V1);
        let loud = put_on_board(&mut state, &reg, &mut ctx, "RH-004", p, Slot::V2);
        let enemy_a = put_on_board(&mut state, &reg, &mut ctx, "MR-001", opp, Slot::V1);
        let enemy_b = put_on_board(&mut state, &reg, &mut ctx, "MR-002", opp, Slot::V2);

        declare_special_attack(&mut state, &reg, &ctx, &mute, &enemy_a, false).unwrap();
        assert!(
            !state
                .card(&enemy_a)
                .unwrap()
                .has_status(StatusEffectType::Taunt)
        );
        declare_special_attack(&mut state, &reg, &ctx, &loud, &enemy_b, false).unwrap();
        assert!(
            state
                .card(&enemy_b)
                .unwrap()
                .has_status(StatusEffectType::Taunt)
        );
        // A support special never opens an attack window either (§8.38).
        assert!(state.pending_attack.is_none());
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

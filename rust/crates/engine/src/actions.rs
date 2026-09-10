//! Legal-action enumeration — port of `getValidActions`
//! (`src/engine/turnManager.ts:829`).
//!
//! The generation order is **load-bearing**: the AI's tie-breaks and every
//! recorded replay depend on it. In the counter window only the defender has
//! actions; otherwise the list is built for the current player during the
//! `main` phase in exactly this sequence:
//!
//! 1. `deployCharacter` — hand order × empty slots in board order
//! 2. `deployShip` — hand order
//! 3. `equipObject` — hand order × own board characters in slot order
//! 4. `playEvent` — hand order
//! 5. `baseSupportAction` — own board characters in slot order
//! 6. `baseAttack` — characters × character targets, then the captain
//! 7. `specialAttack` — same shape
//! 8. `fruitSpecialAttack` — characters × attached awakened fruits
//! 9. `flipCaptain` — one per empty slot
//! 10. `captainAttack` — enemy captain first, then enemy characters
//! 11. `moveCharacter` — characters × adjacent empty slots
//! 12. `activateShip`
//! 13. `awakenFruit`
//! 14. `useHaki { king }`
//! 15. `endTurn` (always last, always present)
#![allow(clippy::collapsible_if)]
// ^ The nested `if` / `if let` blocks in this module mirror the TypeScript
// source branch for branch (see the per-function `PORT:` references). Merging
// them into let-chains would break that 1:1 reading, which is the whole point
// of the port, so the lint is turned off for this file only.

use crate::board::{
    deploy_cost, get_adjacent_slots, get_board_characters, get_effective_atk, get_effective_def,
    get_empty_slots, get_valid_targets, has_summoning_sickness, has_trait,
};
use crate::captain::can_flip_captain;
use crate::combat::{captain_attacker_id, get_eligible_counters};
use crate::error::EngineError;
use crate::fruits::can_awaken_fruit;
use crate::haki::{has_conqueror_in_play, is_haki_available};
use crate::registry::CardRegistry;
use crate::state::GameState;
use crate::types::{
    CardType, GameAction, HakiType, ObjectSubtype, PassiveEffect, Phase, PlayerId, StatusEffect,
    StatusEffectType, Trait,
};

/// TS `getValidActions(state, playerId)` — `src/engine/turnManager.ts:829`.
///
/// **Counter window** (`state.pendingAttack` set): returns nothing unless
/// `playerId` is `getOpponent(state.currentPlayer)`; then the eligible counters
/// ([`crate::combat::get_eligible_counters`], hand order), `passCounter`, the
/// Observation dodge when available and the attack is dodgeable, and one
/// `useShield` per untapped adjacent Bouclier ally (adjacency of the target's
/// slot, or of the captain's slot when the captain is the target).
///
/// **Main phase**: empty unless `playerId == state.currentPlayer` and
/// `state.phase == "main"`, then the 15 groups listed in the module docs.
/// Gating details that must be reproduced exactly:
/// - support / base attacks skip tapped, `usedBaseAction`, summoning-sick and
///   `freeze` / `immobilize` / `sleep` / `loseAction` characters; base attacks
///   additionally require effective ATK > 0 and honour `cannotAttackFemale`;
/// - specials skip `usedSpecialAttack`, once-per-game exhaustion, unaffordable
///   costs and `isSupport` specials, but a `transform` special is offered with
///   the attacker itself as the target;
/// - fruit specials only skip `freeze` / `immobilize` (not `sleep` /
///   `loseAction` — quirk);
/// - the captain may attack when flipped, on-board, untapped, not
///   frozen/immobilised, and either not deployed this turn or `rush`;
/// - `useHaki { king }` needs [`crate::haki::is_haki_available`],
///   [`crate::haki::has_conqueror_in_play`] and at least one enemy at
///   effective DEF ≤ 3.
///
/// Fallible exactly where the TS is: `getCardDef` throws `Card not found: {id}`
/// on an unregistered `defId`, and the unguarded `state.cards[cardId]` of the
/// hand loops raises a TypeError on an id that is not in `state.cards`. Either
/// aborts the whole enumeration in TS, so both surface here as `Err` —
/// enumeration never degrades to a shorter but legal-looking list.
pub fn get_valid_actions(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Result<Vec<GameAction>, EngineError> {
    build_valid_actions(state, registry, player_id)
}

/// Alias of [`get_valid_actions`], kept for callers written against the older
/// `try_` name from when the default entry point swallowed the TS throws.
pub fn try_get_valid_actions(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Result<Vec<GameAction>, EngineError> {
    get_valid_actions(state, registry, player_id)
}

/// TS `statusEffects.some((e) => e.type === "freeze" || ... )` — the four
/// "cannot act" statuses shared by the support / base-attack / special loops.
fn is_action_blocked(effects: &[StatusEffect]) -> bool {
    effects.iter().any(|e| {
        matches!(
            e.effect_type,
            StatusEffectType::Freeze
                | StatusEffectType::Immobilize
                | StatusEffectType::Sleep
                | StatusEffectType::LoseAction
        )
    })
}

/// TS `statusEffects.some((e) => e.type === "freeze" || e.type === "immobilize")`
/// — the narrower test used by the fruit-special loop and by the captain.
fn is_frozen_or_immobilized(effects: &[StatusEffect]) -> bool {
    effects.iter().any(|e| {
        matches!(
            e.effect_type,
            StatusEffectType::Freeze | StatusEffectType::Immobilize
        )
    })
}

/// The shared body of [`get_valid_actions`] / [`try_get_valid_actions`].
///
/// Every definition / instance lookup propagates with `?`, reproducing the TS
/// throw that aborts `getValidActions` as a whole.
fn build_valid_actions(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Result<Vec<GameAction>, EngineError> {
    let mut actions: Vec<GameAction> = Vec::new();
    let player = state.players.get(player_id);

    // ------------------------------------------------------------
    // If there's a pending attack, only counter actions are valid
    // ------------------------------------------------------------
    if let Some(pending) = state.pending_attack.as_ref() {
        let defender_id = state.current_player.opponent();
        if player_id == defender_id {
            let counters = get_eligible_counters(state, registry, player_id)?;
            for id in counters {
                actions.push(GameAction::PlayCounter { instance_id: id });
            }
            // Can always pass
            actions.push(GameAction::PassCounter);

            // Observation Haki to dodge (unless the attack cannot be dodged)
            if is_haki_available(state, player_id, HakiType::Observation)
                && !pending.cannot_be_dodged.unwrap_or(false)
            {
                actions.push(GameAction::UseHaki {
                    haki_type: HakiType::Observation,
                    target_instance_id: None,
                });
            }

            // Bouclier / Shield: an untapped Shield ally adjacent to the target
            // may block.
            if !pending.ignore_shield.unwrap_or(false) {
                let target_slot = if pending.target_is_captain {
                    player.captain.slot
                } else {
                    state.cards.get(&pending.target_id).and_then(|c| c.slot)
                };
                if let Some(target_slot) = target_slot {
                    for adj_slot in get_adjacent_slots(target_slot) {
                        let Some(adj_id) = player.board.get(*adj_slot) else {
                            continue;
                        };
                        if *adj_id == pending.target_id {
                            continue;
                        }
                        let Some(adj_card) = state.cards.get(adj_id) else {
                            continue;
                        };
                        if !adj_card.tapped && has_trait(state, registry, adj_id, Trait::Shield)? {
                            actions.push(GameAction::UseShield {
                                blocker_instance_id: adj_id.clone(),
                            });
                        }
                    }
                }
            }
        }
        return Ok(actions);
    }

    // Only current player can act during main phase
    if player_id != state.current_player {
        return Ok(actions);
    }
    if state.phase != Phase::Main {
        return Ok(actions);
    }

    let opponent_id = player_id.opponent();

    // ------------------------------------------------------------
    // Deploy characters from hand
    // ------------------------------------------------------------
    let empty_slots = get_empty_slots(state, player_id);
    for card_id in &player.hand {
        // TS reads `state.cards[cardId].defId` with no `if (!card)` guard, so a
        // dangling hand id throws a TypeError and aborts the enumeration.
        let card = state
            .cards
            .get(card_id)
            .ok_or_else(|| EngineError::UnknownInstance(card_id.clone()))?;
        let def = registry.get_card_def(&card.def_id)?;
        if def.card_type == CardType::Character {
            let cost = deploy_cost(state, registry, player_id, def)?;
            if state.can_afford(player_id, cost) {
                for slot in &empty_slots {
                    actions.push(GameAction::DeployCharacter {
                        instance_id: card_id.clone(),
                        slot: *slot,
                    });
                }
            }
        }
    }

    // ------------------------------------------------------------
    // Deploy ships from hand
    // ------------------------------------------------------------
    for card_id in &player.hand {
        // TS reads `state.cards[cardId].defId` with no `if (!card)` guard, so a
        // dangling hand id throws a TypeError and aborts the enumeration.
        let card = state
            .cards
            .get(card_id)
            .ok_or_else(|| EngineError::UnknownInstance(card_id.clone()))?;
        let def = registry.get_card_def(&card.def_id)?;
        if def.card_type == CardType::Ship && state.can_afford(player_id, def.cost) {
            actions.push(GameAction::DeployShip {
                instance_id: card_id.clone(),
            });
        }
    }

    // ------------------------------------------------------------
    // Equip objects
    // ------------------------------------------------------------
    let board_chars = get_board_characters(state, player_id);
    for card_id in &player.hand {
        // TS reads `state.cards[cardId].defId` with no `if (!card)` guard, so a
        // dangling hand id throws a TypeError and aborts the enumeration.
        let card = state
            .cards
            .get(card_id)
            .ok_or_else(|| EngineError::UnknownInstance(card_id.clone()))?;
        let def = registry.get_card_def(&card.def_id)?;
        if def.card_type == CardType::Object && state.can_afford(player_id, def.cost) {
            for target in &board_chars {
                actions.push(GameAction::EquipObject {
                    object_instance_id: card_id.clone(),
                    target_instance_id: target.instance_id.clone(),
                });
            }
        }
    }

    // ------------------------------------------------------------
    // Play events
    // ------------------------------------------------------------
    for card_id in &player.hand {
        // TS reads `state.cards[cardId].defId` with no `if (!card)` guard, so a
        // dangling hand id throws a TypeError and aborts the enumeration.
        let card = state
            .cards
            .get(card_id)
            .ok_or_else(|| EngineError::UnknownInstance(card_id.clone()))?;
        let def = registry.get_card_def(&card.def_id)?;
        if def.card_type == CardType::Event && state.can_afford(player_id, def.cost) {
            actions.push(GameAction::PlayEvent {
                instance_id: card_id.clone(),
                targets: None,
            });
        }
    }

    // ------------------------------------------------------------
    // Support base actions (Usopp trap, Robin immobilize, Chopper heal,
    // Brook buff, etc.)
    // ------------------------------------------------------------
    let opp_chars = get_board_characters(state, opponent_id);
    for ch in &board_chars {
        if ch.tapped || ch.used_base_action {
            continue;
        }
        if has_summoning_sickness(state, registry, &ch.instance_id)? {
            continue;
        }
        if is_action_blocked(&ch.status_effects) {
            continue;
        }

        let def = registry.get_card_def(&ch.def_id)?;
        let Some(ba) = def.base_action.as_ref() else {
            continue;
        };
        if !ba.is_support.unwrap_or(false) {
            continue;
        }

        if ba.scry.is_some_and(|n| n != 0) {
            // No target needed (reorder your own deck top).
            actions.push(GameAction::BaseSupportAction {
                instance_id: ch.instance_id.clone(),
                target_instance_id: None,
            });
        } else if ba.bluff.unwrap_or(false) {
            // Target: an enemy of effective DEF <= 1.
            for opp in &opp_chars {
                if get_effective_def(state, registry, &opp.instance_id)? <= 1 {
                    actions.push(GameAction::BaseSupportAction {
                        instance_id: ch.instance_id.clone(),
                        target_instance_id: Some(opp.instance_id.clone()),
                    });
                }
            }
        } else if ba.buff_ally_atk.is_some_and(|n| n != 0) {
            // Target: any ally (including self).
            for ally in &board_chars {
                actions.push(GameAction::BaseSupportAction {
                    instance_id: ch.instance_id.clone(),
                    target_instance_id: Some(ally.instance_id.clone()),
                });
            }
        } else if ba.immobilize.unwrap_or(false) {
            // Target: any enemy character
            for opp in &opp_chars {
                actions.push(GameAction::BaseSupportAction {
                    instance_id: ch.instance_id.clone(),
                    target_instance_id: Some(opp.instance_id.clone()),
                });
            }
        } else if ba
            .description
            .as_ref()
            .is_some_and(|d| d.contains("piege") || d.contains("Piege"))
        {
            // Trap: target any enemy character
            for opp in &opp_chars {
                actions.push(GameAction::BaseSupportAction {
                    instance_id: ch.instance_id.clone(),
                    target_instance_id: Some(opp.instance_id.clone()),
                });
            }
        } else if ba.heal_amount.is_some_and(|n| n != 0) {
            // Heal: target any friendly character on board
            for ally in &board_chars {
                if ally.instance_id == ch.instance_id {
                    continue;
                }
                actions.push(GameAction::BaseSupportAction {
                    instance_id: ch.instance_id.clone(),
                    target_instance_id: Some(ally.instance_id.clone()),
                });
            }
        } else {
            // Global buff (no specific target needed)
            actions.push(GameAction::BaseSupportAction {
                instance_id: ch.instance_id.clone(),
                target_instance_id: None,
            });
        }
    }

    // ------------------------------------------------------------
    // Base attacks — ALL characters with ATK > 0 can base attack
    // (even support chars like Chopper ATK 1 — they do their effect + attack)
    // ------------------------------------------------------------
    for ch in &board_chars {
        if ch.tapped || ch.used_base_action {
            continue;
        }
        if has_summoning_sickness(state, registry, &ch.instance_id)? {
            continue;
        }

        let def = registry.get_card_def(&ch.def_id)?;
        // Must have effective ATK > 0 to attack (includes equipment + modifiers)
        if get_effective_atk(state, registry, &ch.instance_id)? <= 0 {
            continue;
        }

        if is_action_blocked(&ch.status_effects) {
            continue;
        }

        // Check cannotAttackFemale passive
        let cannot_attack_female = def.passive.as_ref().is_some_and(|p| {
            p.effects
                .iter()
                .any(|e| matches!(e, PassiveEffect::CannotAttackFemale))
        });

        let targets = get_valid_targets(state, registry, &ch.instance_id, false)?;
        for target_id in &targets.character_targets {
            // Filter female targets if cannotAttackFemale
            if cannot_attack_female {
                if let Some(target_card) = state.cards.get(target_id) {
                    let target_def = registry.get_card_def(&target_card.def_id)?;
                    if target_def.has_tag("female") {
                        continue;
                    }
                }
            }
            actions.push(GameAction::BaseAttack {
                attacker_instance_id: ch.instance_id.clone(),
                target_instance_id: target_id.clone(),
                target_is_captain: None,
            });
        }
        if targets.can_target_captain {
            actions.push(GameAction::BaseAttack {
                attacker_instance_id: ch.instance_id.clone(),
                target_instance_id: captain_attacker_id(opponent_id),
                target_is_captain: Some(true),
            });
        }
    }

    // ------------------------------------------------------------
    // Special attacks (all characters with specials, including support chars)
    // ------------------------------------------------------------
    for ch in &board_chars {
        if ch.used_special_attack {
            continue;
        }
        if has_summoning_sickness(state, registry, &ch.instance_id)? {
            continue;
        }

        let def = registry.get_card_def(&ch.def_id)?;
        let Some(sa) = def.special_attack.as_ref() else {
            continue;
        };
        if sa.once_per_game.unwrap_or(false) && ch.used_once(&sa.name) {
            continue;
        }
        if !state.can_afford(player_id, sa.cost) {
            continue;
        }
        if is_action_blocked(&ch.status_effects) {
            continue;
        }

        // Self-transformation special (Chopper Monster Point): target self, no
        // enemy needed.
        if sa.transform.is_some() {
            actions.push(GameAction::SpecialAttack {
                attacker_instance_id: ch.instance_id.clone(),
                target_instance_id: ch.instance_id.clone(),
                target_is_captain: None,
            });
            continue;
        }
        // Other support specials (heal/buff with no target) — skip offering for now.
        if sa.is_support.unwrap_or(false) {
            continue;
        }

        // Check cannotAttackFemale
        let cant_female = def.passive.as_ref().is_some_and(|p| {
            p.effects
                .iter()
                .any(|e| matches!(e, PassiveEffect::CannotAttackFemale))
        });

        let targets = get_valid_targets(state, registry, &ch.instance_id, true)?;
        for target_id in &targets.character_targets {
            if cant_female {
                if let Some(tc) = state.cards.get(target_id) {
                    let tdef = registry.get_card_def(&tc.def_id)?;
                    if tdef.has_tag("female") {
                        continue;
                    }
                }
            }
            actions.push(GameAction::SpecialAttack {
                attacker_instance_id: ch.instance_id.clone(),
                target_instance_id: target_id.clone(),
                target_is_captain: None,
            });
        }
        if targets.can_target_captain {
            actions.push(GameAction::SpecialAttack {
                attacker_instance_id: ch.instance_id.clone(),
                target_instance_id: captain_attacker_id(opponent_id),
                target_is_captain: Some(true),
            });
        }
    }

    // ------------------------------------------------------------
    // Fruit awakening special attacks
    // ------------------------------------------------------------
    for ch in &board_chars {
        if has_summoning_sickness(state, registry, &ch.instance_id)? {
            continue;
        }
        if is_frozen_or_immobilized(&ch.status_effects) {
            continue;
        }

        for obj_id in &ch.attached_objects {
            let Some(obj_card) = state.cards.get(obj_id) else {
                continue;
            };
            if !obj_card.is_awakened.unwrap_or(false) {
                continue;
            }
            let obj_def = registry.get_card_def(&obj_card.def_id)?;
            let Some(fruit_spec) = obj_def
                .fruit_effects
                .as_ref()
                .and_then(|fx| fx.awakening.as_ref())
                .and_then(|aw| aw.special_attack.as_ref())
            else {
                continue;
            };
            if fruit_spec.once_per_game.unwrap_or(false) && ch.used_once(&fruit_spec.name) {
                continue;
            }
            if !state.can_afford(player_id, fruit_spec.cost) {
                continue;
            }

            let targets = get_valid_targets(state, registry, &ch.instance_id, true)?;
            for target_id in &targets.character_targets {
                actions.push(GameAction::FruitSpecialAttack {
                    attacker_instance_id: ch.instance_id.clone(),
                    fruit_instance_id: obj_id.clone(),
                    target_instance_id: target_id.clone(),
                    target_is_captain: None,
                });
            }
            if targets.can_target_captain {
                actions.push(GameAction::FruitSpecialAttack {
                    attacker_instance_id: ch.instance_id.clone(),
                    fruit_instance_id: obj_id.clone(),
                    target_instance_id: captain_attacker_id(opponent_id),
                    target_is_captain: Some(true),
                });
            }
        }
    }

    // ------------------------------------------------------------
    // Captain flip
    // ------------------------------------------------------------
    if can_flip_captain(state, registry, player_id)? {
        for slot in &empty_slots {
            actions.push(GameAction::FlipCaptain { slot: *slot });
        }
    }

    // ------------------------------------------------------------
    // Captain attacks (if verso and on board) — frozen/immobilized captains
    // can't act
    // ------------------------------------------------------------
    let captain_disabled = is_frozen_or_immobilized(&player.captain.status_effects);
    if player.captain.flipped
        && player.captain.slot.is_some()
        && !player.captain.tapped
        && !captain_disabled
    {
        // TS `getCaptainDef(...)` — unguarded, so an unregistered captain
        // aborts the enumeration instead of dropping the attack group.
        let cap_def = registry.get_captain_def(&player.captain.def_id)?;
        let has_rush = cap_def
            .verso
            .traits
            .as_ref()
            .is_some_and(|ts| ts.contains(&Trait::Rush));
        if player.captain.deployed_turn != Some(i64::from(state.turn_number)) || has_rush {
            // Can attack — simplified: target any enemy front or captain
            actions.push(GameAction::CaptainAttack {
                target_instance_id: captain_attacker_id(opponent_id),
                target_is_captain: Some(true),
                is_special: None,
            });
            for opp in &opp_chars {
                actions.push(GameAction::CaptainAttack {
                    target_instance_id: opp.instance_id.clone(),
                    target_is_captain: None,
                    is_special: None,
                });
            }
        }
    }

    // ------------------------------------------------------------
    // Free move
    // ------------------------------------------------------------
    if !player.used_free_move {
        for ch in &board_chars {
            if let Some(slot) = ch.slot {
                for adj_slot in get_adjacent_slots(slot) {
                    if player.board.get(*adj_slot).is_none() {
                        actions.push(GameAction::MoveCharacter {
                            instance_id: ch.instance_id.clone(),
                            target_slot: *adj_slot,
                        });
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------
    // Activate ship ability
    // ------------------------------------------------------------
    if let Some(active_ship) = player.active_ship.as_ref() {
        if let Some(ship_card) = state.cards.get(active_ship) {
            let ship_def = registry.get_card_def(&ship_card.def_id)?;
            if let Some(sa) = ship_def.ship_active.as_ref() {
                let can_use = (!sa.once_per_game.unwrap_or(false)
                    || !ship_card.used_once(&sa.name))
                    && state.can_afford(player_id, sa.cost);
                if can_use {
                    actions.push(GameAction::ActivateShip {
                        ship_instance_id: active_ship.clone(),
                    });
                }
            }
        }
    }

    // ------------------------------------------------------------
    // Awaken Devil Fruits
    // ------------------------------------------------------------
    for ch in &board_chars {
        for obj_id in &ch.attached_objects {
            if let Some(obj_card) = state.cards.get(obj_id) {
                let obj_def = registry.get_card_def(&obj_card.def_id)?;
                if obj_def.subtype == Some(ObjectSubtype::Fruit)
                    && can_awaken_fruit(state, registry, player_id, obj_id)?
                {
                    actions.push(GameAction::AwakenFruit {
                        fruit_instance_id: obj_id.clone(),
                    });
                }
            }
        }
    }

    // Armament Haki (T7+) is a passive in Rulebook v3.1 — no action to offer.

    // ------------------------------------------------------------
    // Roi Haki (T10+): requires a Conquerant unit in play, KOs all enemies
    // DEF <= 3, 1x/game.
    // ------------------------------------------------------------
    if is_haki_available(state, player_id, HakiType::King)
        && has_conqueror_in_play(state, registry, player_id)?
    {
        let mut has_target = false;
        for c in &opp_chars {
            if get_effective_def(state, registry, &c.instance_id)? <= 3 {
                has_target = true;
                break;
            }
        }
        if has_target {
            actions.push(GameAction::UseHaki {
                haki_type: HakiType::King,
                target_instance_id: None,
            });
        }
    }

    // End turn is always valid
    actions.push(GameAction::EndTurn);

    Ok(actions)
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Board, CaptainInstance, CardInstance, PlayerState, Players};
    use crate::types::{
        BaseAction, CaptainDef, CaptainRecto, CaptainVerso, CardDef, EntryEffect, Faction,
        FlipCondition, FruitAwakening, FruitAwakeningSpecialAttack, FruitBaseEffects, FruitEffects,
        PassiveDef, Rarity, ShipActive, Slot, SpecialAttack, StatusEffect, Transform, Zone,
    };
    use std::collections::BTreeMap;

    const P1: PlayerId = PlayerId::Player1;
    const P2: PlayerId = PlayerId::Player2;

    fn captain_def() -> CaptainDef {
        let passive = PassiveDef {
            name: "cap".into(),
            description: "d".into(),
            effects: vec![],
        };
        CaptainDef {
            id: "CAP-T".into(),
            name: "Test Captain".into(),
            faction: Faction::Pirate,
            tags: None,
            traits: None,
            recto: CaptainRecto {
                pv: 20,
                atk: 3,
                def: 2,
                passive: passive.clone(),
                attacks: vec![],
                surcharge: None,
            },
            // No cost / no free-flip clause => `canFlipCaptain` is false, so the
            // `flipCaptain` group stays empty in these fixtures.
            flip_condition: FlipCondition::default(),
            verso: CaptainVerso {
                pv: 25,
                atk: 5,
                def: 3,
                passive,
                entry_effect: EntryEffect::Draw { amount: 0 },
                base_action: BaseAction {
                    name: "b".into(),
                    atk: 5,
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

    fn character(id: &str, cost: i32, atk: i32, def: i32, pv: i32) -> CardDef {
        let mut d = CardDef::new(
            id,
            format!("Char {id}"),
            CardType::Character,
            cost,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        d.atk = Some(atk);
        d.def = Some(def);
        d.pv = Some(pv);
        d
    }

    fn simple(id: &str, t: CardType, cost: i32) -> CardDef {
        CardDef::new(
            id,
            format!("Card {id}"),
            t,
            cost,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        )
    }

    fn player_state(id: PlayerId) -> PlayerState {
        PlayerState {
            id,
            captain: CaptainInstance::new("CAP-T".into(), id, 20),
            deck: Vec::new(),
            hand: Vec::new(),
            graveyard: Vec::new(),
            board: Board::empty(),
            active_ship: None,
            volonte: 20,
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

    fn blank_state() -> GameState {
        GameState {
            cards: BTreeMap::new(),
            players: Players {
                player1: player_state(P1),
                player2: player_state(P2),
            },
            turn_number: 3,
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
        zone: Zone,
        slot: Option<Slot>,
    ) -> String {
        let n = state.cards.len();
        let iid = format!("{def_id}#{n}");
        let pv = registry.card_def(def_id).and_then(|d| d.pv).unwrap_or(0);
        let mut inst = CardInstance::new(iid.clone(), def_id.to_string(), owner, pv);
        inst.zone = zone;
        inst.slot = slot;
        state.cards.insert(iid.clone(), inst);
        match zone {
            Zone::Hand => state.players.get_mut(owner).hand.push(iid.clone()),
            Zone::Deck => state.players.get_mut(owner).deck.push(iid.clone()),
            Zone::Board => {
                if let Some(s) = slot {
                    state.players.get_mut(owner).board.set(s, Some(iid.clone()));
                }
            }
            _ => {}
        }
        iid
    }

    fn registry_with(cards: Vec<CardDef>) -> CardRegistry {
        CardRegistry::from_sets([cards], [captain_def()])
    }

    fn types_of(actions: &[GameAction]) -> Vec<&'static str> {
        actions.iter().map(|a| a.type_name()).collect()
    }

    /// Test shim: every state below has registered defs and live instances, so
    /// the enumeration cannot fail — the tests that *do* exercise the TS throw
    /// call [`super::get_valid_actions`] directly.
    fn get_valid_actions(
        state: &GameState,
        registry: &CardRegistry,
        player_id: PlayerId,
    ) -> Vec<GameAction> {
        super::get_valid_actions(state, registry, player_id).expect("valid state")
    }

    // --- early returns (no dependency on the other modules) ---

    #[test]
    fn a_pending_attack_gives_the_attacker_nothing() {
        let reg = registry_with(vec![character("A", 1, 2, 1, 3)]);
        let mut state = blank_state();
        let a = put(&mut state, &reg, "A", P1, Zone::Board, Some(Slot::V1));
        state.pending_attack = Some(crate::state::PendingAttack {
            attacker_id: a.clone(),
            target_id: "x".into(),
            target_is_captain: false,
            is_special: false,
            raw_damage: 2,
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
        });
        // The attacker is `currentPlayer`; only `getOpponent(currentPlayer)` acts.
        assert!(get_valid_actions(&state, &reg, P1).is_empty());
    }

    #[test]
    fn only_the_current_player_acts_and_only_in_main() {
        let reg = registry_with(vec![character("A", 1, 2, 1, 3)]);
        let mut state = blank_state();
        assert!(get_valid_actions(&state, &reg, P2).is_empty());
        state.phase = Phase::Draw;
        assert!(get_valid_actions(&state, &reg, P1).is_empty());
    }

    // --- counter window ---

    #[test]
    fn counter_window_lists_counters_then_pass_then_shield() {
        let mut shield_char = character("SHIELD", 1, 1, 1, 3);
        shield_char.traits = Some(vec![Trait::Shield]);
        let mut counter = simple("CTR", CardType::Counter, 0);
        counter.counter_effect = Some(crate::types::CounterEffect::Cancel {
            description: "Annule".into(),
            max_attacker_atk: None,
            self_captain_damage: None,
            once: None,
        });
        let reg = registry_with(vec![character("A", 1, 2, 1, 3), shield_char, counter]);

        let mut state = blank_state();
        // P1 attacks P2's V2 character; P2 has a Shield ally in V1 (adjacent to V2).
        let atk = put(&mut state, &reg, "A", P1, Zone::Board, Some(Slot::V1));
        let target = put(&mut state, &reg, "A", P2, Zone::Board, Some(Slot::V2));
        let blocker = put(&mut state, &reg, "SHIELD", P2, Zone::Board, Some(Slot::V1));
        let ctr = put(&mut state, &reg, "CTR", P2, Zone::Hand, None);

        state.pending_attack = Some(crate::state::PendingAttack {
            attacker_id: atk,
            target_id: target.clone(),
            target_is_captain: false,
            is_special: false,
            raw_damage: 2,
            attack_power: Some(2),
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
        });

        let actions = get_valid_actions(&state, &reg, P2);
        assert_eq!(
            actions,
            vec![
                GameAction::PlayCounter { instance_id: ctr },
                GameAction::PassCounter,
                // Observation is locked before turn 5 => no `useHaki`.
                GameAction::UseShield {
                    blocker_instance_id: blocker
                },
            ]
        );
    }

    #[test]
    fn ignore_shield_suppresses_the_block() {
        let mut shield_char = character("SHIELD", 1, 1, 1, 3);
        shield_char.traits = Some(vec![Trait::Shield]);
        let reg = registry_with(vec![character("A", 1, 2, 1, 3), shield_char]);
        let mut state = blank_state();
        let atk = put(&mut state, &reg, "A", P1, Zone::Board, Some(Slot::V1));
        let target = put(&mut state, &reg, "A", P2, Zone::Board, Some(Slot::V2));
        put(&mut state, &reg, "SHIELD", P2, Zone::Board, Some(Slot::V1));
        state.pending_attack = Some(crate::state::PendingAttack {
            attacker_id: atk,
            target_id: target,
            target_is_captain: false,
            is_special: true,
            raw_damage: 2,
            attack_power: Some(2),
            element: None,
            attack_traits: Vec::new(),
            has_haki: false,
            ignore_shield: Some(true),
            cannot_be_dodged: None,
            immobilize: None,
            sleep: None,
            pushback: None,
            pushback_slots: None,
            strip_stealth: None,
            survive_played: None,
        });
        assert_eq!(
            types_of(&get_valid_actions(&state, &reg, P2)),
            vec!["passCounter"]
        );
    }

    #[test]
    fn observation_dodge_appears_after_pass_and_respects_cannot_be_dodged() {
        let reg = registry_with(vec![character("A", 1, 2, 1, 3)]);
        let mut state = blank_state();
        state.turn_number = 6; // >= the Observation threshold (5)
        let atk = put(&mut state, &reg, "A", P1, Zone::Board, Some(Slot::V1));
        let target = put(&mut state, &reg, "A", P2, Zone::Board, Some(Slot::V1));
        let pending = crate::state::PendingAttack {
            attacker_id: atk,
            target_id: target,
            target_is_captain: false,
            is_special: false,
            raw_damage: 2,
            attack_power: Some(2),
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
        };
        state.pending_attack = Some(pending.clone());
        assert_eq!(
            get_valid_actions(&state, &reg, P2),
            vec![
                GameAction::PassCounter,
                GameAction::UseHaki {
                    haki_type: HakiType::Observation,
                    target_instance_id: None,
                },
            ]
        );

        // `cannotBeDodged` removes it.
        state.pending_attack = Some(crate::state::PendingAttack {
            cannot_be_dodged: Some(true),
            ..pending.clone()
        });
        assert_eq!(
            get_valid_actions(&state, &reg, P2),
            vec![GameAction::PassCounter]
        );

        // So does having already dodged this turn.
        state.pending_attack = Some(pending);
        state.players.get_mut(P2).observation_used = true;
        assert_eq!(
            get_valid_actions(&state, &reg, P2),
            vec![GameAction::PassCounter]
        );
    }

    // --- main phase ordering ---

    #[test]
    fn main_phase_group_order_matches_the_ts() {
        let reg = registry_with(vec![
            character("A", 1, 2, 1, 3),
            // ATK 0 so the on-board ally contributes no base attack.
            character("MUTE", 1, 0, 1, 3),
            simple("SHIP", CardType::Ship, 2),
            simple("OBJ", CardType::Object, 1),
            simple("EVT", CardType::Event, 1),
        ]);
        let mut state = blank_state();
        let ally = put(&mut state, &reg, "MUTE", P1, Zone::Board, Some(Slot::V1));
        let hand_char = put(&mut state, &reg, "A", P1, Zone::Hand, None);
        let hand_ship = put(&mut state, &reg, "SHIP", P1, Zone::Hand, None);
        let hand_obj = put(&mut state, &reg, "OBJ", P1, Zone::Hand, None);
        let hand_evt = put(&mut state, &reg, "EVT", P1, Zone::Hand, None);

        let actions = get_valid_actions(&state, &reg, P1);
        assert_eq!(
            types_of(&actions),
            vec![
                // 5 empty slots x 1 character in hand, in board order
                "deployCharacter",
                "deployCharacter",
                "deployCharacter",
                "deployCharacter",
                "deployCharacter",
                "deployShip",
                "equipObject",
                "playEvent",
                // V1 -> V2 and V1 -> A1
                "moveCharacter",
                "moveCharacter",
                "endTurn",
            ]
        );
        assert_eq!(
            actions[0],
            GameAction::DeployCharacter {
                instance_id: hand_char.clone(),
                slot: Slot::V2
            }
        );
        assert_eq!(
            actions[4],
            GameAction::DeployCharacter {
                instance_id: hand_char,
                slot: Slot::A3
            }
        );
        assert_eq!(
            actions[5],
            GameAction::DeployShip {
                instance_id: hand_ship
            }
        );
        assert_eq!(
            actions[6],
            GameAction::EquipObject {
                object_instance_id: hand_obj,
                target_instance_id: ally.clone()
            }
        );
        assert_eq!(
            actions[7],
            GameAction::PlayEvent {
                instance_id: hand_evt,
                targets: None
            }
        );
        assert_eq!(
            actions[8],
            GameAction::MoveCharacter {
                instance_id: ally,
                target_slot: Slot::V2
            }
        );
    }

    // --- base attacks ---

    #[test]
    fn base_attack_skips_female_targets_for_cannot_attack_female() {
        let mut attacker = character("ATK", 1, 3, 1, 3);
        attacker.passive = Some(PassiveDef {
            name: "p".into(),
            description: "d".into(),
            effects: vec![PassiveEffect::CannotAttackFemale],
        });
        let mut female = character("FEM", 1, 1, 1, 3);
        female.tags = Some(vec!["female".into()]);
        let reg = registry_with(vec![attacker, female, character("MALE", 1, 1, 1, 3)]);

        let mut state = blank_state();
        let a = put(&mut state, &reg, "ATK", P1, Zone::Board, Some(Slot::V1));
        put(&mut state, &reg, "FEM", P2, Zone::Board, Some(Slot::V1));
        let male = put(&mut state, &reg, "MALE", P2, Zone::Board, Some(Slot::V2));

        let attacks: Vec<GameAction> = get_valid_actions(&state, &reg, P1)
            .into_iter()
            .filter(|x| matches!(x, GameAction::BaseAttack { .. }))
            .collect();
        assert_eq!(
            attacks,
            vec![GameAction::BaseAttack {
                attacker_instance_id: a,
                target_instance_id: male,
                target_is_captain: None,
            }]
        );
    }

    #[test]
    fn base_attack_needs_effective_atk_above_zero_and_a_free_action() {
        let reg = registry_with(vec![
            character("ATK", 1, 3, 1, 3),
            character("MUTE", 1, 0, 1, 3),
            character("DUMMY", 1, 1, 1, 3),
        ]);
        let mut state = blank_state();
        let a = put(&mut state, &reg, "ATK", P1, Zone::Board, Some(Slot::V1));
        put(&mut state, &reg, "MUTE", P1, Zone::Board, Some(Slot::V2));
        put(&mut state, &reg, "DUMMY", P2, Zone::Board, Some(Slot::V1));

        let n = |s: &GameState| {
            get_valid_actions(s, &reg, P1)
                .into_iter()
                .filter(|x| matches!(x, GameAction::BaseAttack { .. }))
                .count()
        };
        // Only the ATK 3 character attacks (the ATK 0 one is skipped).
        assert_eq!(n(&state), 1);

        // Tapped / usedBaseAction / status effects all remove it.
        state.cards.get_mut(&a).unwrap().tapped = true;
        assert_eq!(n(&state), 0);
        state.cards.get_mut(&a).unwrap().tapped = false;
        state.cards.get_mut(&a).unwrap().used_base_action = true;
        assert_eq!(n(&state), 0);
        state.cards.get_mut(&a).unwrap().used_base_action = false;
        state.cards.get_mut(&a).unwrap().status_effects = vec![StatusEffect {
            effect_type: StatusEffectType::Sleep,
            turns_remaining: 1,
            damage_per_turn: 0,
            source: "test".into(),
        }];
        assert_eq!(n(&state), 0);
    }

    // --- specials ---

    #[test]
    fn a_transform_special_targets_the_attacker_itself() {
        let mut c = character("TR", 1, 1, 1, 3);
        c.special_attack = Some(SpecialAttack {
            name: "Monster Point".into(),
            cost: 2,
            atk_bonus: 0,
            transform: Some(Transform {
                atk: 9,
                def: 3,
                pv: 9,
                turns: 2,
            }),
            ..Default::default()
        });
        let reg = registry_with(vec![c]);
        let mut state = blank_state();
        let a = put(&mut state, &reg, "TR", P1, Zone::Board, Some(Slot::V1));
        let specials: Vec<GameAction> = get_valid_actions(&state, &reg, P1)
            .into_iter()
            .filter(|x| matches!(x, GameAction::SpecialAttack { .. }))
            .collect();
        assert_eq!(
            specials,
            vec![GameAction::SpecialAttack {
                attacker_instance_id: a.clone(),
                target_instance_id: a,
                target_is_captain: None,
            }]
        );
    }

    // --- fruit specials (the documented quirk) ---

    fn awakened_fruit_def() -> CardDef {
        // `subtype` is deliberately NOT `fruit`: the `awakenFruit` group calls
        // `canAwakenFruit`, which is out of scope here — the fruit-special
        // group only reads `isAwakened` + `fruitEffects`.
        let mut d = simple("FRUIT", CardType::Object, 1);
        d.subtype = Some(ObjectSubtype::Weapon);
        d.fruit_effects = Some(FruitEffects {
            base: FruitBaseEffects::default(),
            awakening: Some(FruitAwakening {
                porteur_legitime: "Char".into(),
                min_turns: 1,
                vol_cost: 1,
                grants_traits: None,
                atk_bonus: None,
                def_bonus: None,
                passive_description: None,
                special_attack: Some(FruitAwakeningSpecialAttack {
                    name: "Eveil".into(),
                    cost: 1,
                    atk_bonus: 3,
                    description: "d".into(),
                    once_per_game: None,
                    attack_traits: None,
                    element: None,
                    ignore_def: None,
                    immobilize: None,
                    sleep: None,
                    pushback: None,
                    ignore_shield: None,
                    strip_stealth: None,
                }),
            }),
        });
        d
    }

    #[test]
    fn fruit_specials_ignore_sleep_but_not_freeze() {
        let reg = registry_with(vec![
            character("A", 1, 2, 1, 3),
            character("DUMMY", 1, 1, 1, 3),
            awakened_fruit_def(),
        ]);
        let mut state = blank_state();
        let bearer = put(&mut state, &reg, "A", P1, Zone::Board, Some(Slot::V1));
        let fruit = put(&mut state, &reg, "FRUIT", P1, Zone::Board, None);
        let enemy = put(&mut state, &reg, "DUMMY", P2, Zone::Board, Some(Slot::V1));
        state.cards.get_mut(&fruit).unwrap().is_awakened = Some(true);
        state
            .cards
            .get_mut(&bearer)
            .unwrap()
            .attached_objects
            .push(fruit.clone());

        let fruit_specials = |s: &GameState| -> Vec<GameAction> {
            get_valid_actions(s, &reg, P1)
                .into_iter()
                .filter(|x| matches!(x, GameAction::FruitSpecialAttack { .. }))
                .collect()
        };

        assert_eq!(
            fruit_specials(&state),
            vec![GameAction::FruitSpecialAttack {
                attacker_instance_id: bearer.clone(),
                fruit_instance_id: fruit.clone(),
                target_instance_id: enemy,
                target_is_captain: None,
            }]
        );

        // `sleep` blocks base attacks but NOT fruit specials (TS quirk).
        state.cards.get_mut(&bearer).unwrap().status_effects = vec![StatusEffect {
            effect_type: StatusEffectType::Sleep,
            turns_remaining: 1,
            damage_per_turn: 0,
            source: "test".into(),
        }];
        assert_eq!(fruit_specials(&state).len(), 1);

        // `freeze` does block them.
        state.cards.get_mut(&bearer).unwrap().status_effects = vec![StatusEffect {
            effect_type: StatusEffectType::Freeze,
            turns_remaining: 1,
            damage_per_turn: 0,
            source: "test".into(),
        }];
        assert!(fruit_specials(&state).is_empty());
    }

    // --- captain attacks ---

    #[test]
    fn captain_attacks_list_the_enemy_captain_first() {
        let reg = registry_with(vec![character("A", 1, 2, 1, 3)]);
        let mut state = blank_state();
        let e1 = put(&mut state, &reg, "A", P2, Zone::Board, Some(Slot::V1));
        let e2 = put(&mut state, &reg, "A", P2, Zone::Board, Some(Slot::V2));
        {
            let cap = &mut state.players.get_mut(P1).captain;
            cap.flipped = true;
            cap.slot = Some(Slot::V2);
            cap.deployed_turn = Some(1); // not this turn (turn 3)
        }
        let caps: Vec<GameAction> = get_valid_actions(&state, &reg, P1)
            .into_iter()
            .filter(|x| matches!(x, GameAction::CaptainAttack { .. }))
            .collect();
        assert_eq!(
            caps,
            vec![
                GameAction::CaptainAttack {
                    target_instance_id: "captain_player2".into(),
                    target_is_captain: Some(true),
                    is_special: None,
                },
                GameAction::CaptainAttack {
                    target_instance_id: e1,
                    target_is_captain: None,
                    is_special: None,
                },
                GameAction::CaptainAttack {
                    target_instance_id: e2,
                    target_is_captain: None,
                    is_special: None,
                },
            ]
        );

        // Summoning-sick (deployed this turn, no Rush) => no captain attack.
        state.players.get_mut(P1).captain.deployed_turn = Some(i64::from(state.turn_number));
        assert!(
            !get_valid_actions(&state, &reg, P1)
                .iter()
                .any(|x| matches!(x, GameAction::CaptainAttack { .. }))
        );
    }

    // --- support actions ---

    #[test]
    fn support_branches_pick_their_targets_in_ts_order() {
        let mut healer = character("HEAL", 1, 0, 1, 3);
        healer.base_action = Some(BaseAction {
            name: "Soin".into(),
            atk: 0,
            is_support: Some(true),
            heal_amount: Some(2),
            ..Default::default()
        });
        let mut trapper = character("TRAP", 1, 0, 1, 3);
        trapper.base_action = Some(BaseAction {
            name: "Piege".into(),
            atk: 0,
            is_support: Some(true),
            description: Some("Pose un Piege sur un ennemi".into()),
            ..Default::default()
        });
        let reg = registry_with(vec![healer, trapper, character("DUMMY", 1, 1, 1, 3)]);
        let mut state = blank_state();
        let h = put(&mut state, &reg, "HEAL", P1, Zone::Board, Some(Slot::V1));
        let t = put(&mut state, &reg, "TRAP", P1, Zone::Board, Some(Slot::V2));
        let enemy = put(&mut state, &reg, "DUMMY", P2, Zone::Board, Some(Slot::V1));

        let supports: Vec<GameAction> = get_valid_actions(&state, &reg, P1)
            .into_iter()
            .filter(|x| matches!(x, GameAction::BaseSupportAction { .. }))
            .collect();
        assert_eq!(
            supports,
            vec![
                // Heal: every ally except itself.
                GameAction::BaseSupportAction {
                    instance_id: h,
                    target_instance_id: Some(t.clone()),
                },
                // Trap (description contains "Piege"): every enemy.
                GameAction::BaseSupportAction {
                    instance_id: t,
                    target_instance_id: Some(enemy),
                },
            ]
        );
    }

    // --- strict vs. lenient def lookup ---

    #[test]
    fn an_unknown_def_id_throws() {
        let reg = registry_with(vec![character("A", 1, 2, 1, 3)]);
        let mut state = blank_state();
        // A hand card whose definition is missing from the registry: the TS
        // `getCardDef` would throw here.
        let ghost = "GHOST#99".to_string();
        state.cards.insert(
            ghost.clone(),
            CardInstance::new(ghost.clone(), "GHOST".into(), P1, 1),
        );
        state.players.get_mut(P1).hand.push(ghost);
        let good = put(&mut state, &reg, "A", P1, Zone::Hand, None);

        // TS: `getCardDef` throws out of `getValidActions`, so the caller gets
        // an exception rather than a list that quietly omits the ghost.
        assert_eq!(
            super::get_valid_actions(&state, &reg, P1)
                .unwrap_err()
                .to_string(),
            "Card not found: GHOST"
        );
        assert!(try_get_valid_actions(&state, &reg, P1).is_err());
        // Without the ghost the very same state enumerates normally.
        state.players.get_mut(P1).hand.retain(|id| id == &good);
        let actions = get_valid_actions(&state, &reg, P1);
        assert!(actions.iter().all(|a| match a {
            GameAction::DeployCharacter { instance_id, .. } => *instance_id == good,
            _ => true,
        }));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, GameAction::DeployCharacter { .. }))
        );
    }

    #[test]
    fn a_dangling_hand_id_throws() {
        // TS reads `state.cards[cardId].defId` with no guard, so a hand id with
        // no instance behind it throws a TypeError and the whole enumeration
        // is abandoned.
        let reg = registry_with(vec![character("A", 1, 2, 1, 3)]);
        let mut state = blank_state();
        state.players.get_mut(P1).hand.push("nowhere".into());
        let good = put(&mut state, &reg, "A", P1, Zone::Hand, None);

        assert_eq!(
            super::get_valid_actions(&state, &reg, P1)
                .unwrap_err()
                .to_string(),
            "Instance not found: nowhere"
        );
        // Drop the dangling id and the good card is offered as before.
        state.players.get_mut(P1).hand.retain(|id| id == &good);
        let actions = get_valid_actions(&state, &reg, P1);
        assert!(actions.iter().any(|a| matches!(
            a,
            GameAction::DeployCharacter { instance_id, .. } if *instance_id == good
        )));
    }

    // --- ship activation ---

    #[test]
    fn ship_activation_respects_once_per_game_and_cost() {
        let mut ship = simple("SHIP", CardType::Ship, 2);
        ship.ship_active = Some(ShipActive {
            name: "Canon".into(),
            cost: 3,
            description: "d".into(),
            once_per_game: Some(true),
        });
        let reg = registry_with(vec![ship]);
        let mut state = blank_state();
        let s = put(&mut state, &reg, "SHIP", P1, Zone::Board, None);
        state.players.get_mut(P1).active_ship = Some(s.clone());

        let has_activate = |st: &GameState| {
            get_valid_actions(st, &reg, P1)
                .iter()
                .any(|x| matches!(x, GameAction::ActivateShip { .. }))
        };
        assert!(has_activate(&state));

        state
            .cards
            .get_mut(&s)
            .unwrap()
            .used_once_abilities
            .push("Canon".into());
        assert!(!has_activate(&state));

        state.cards.get_mut(&s).unwrap().used_once_abilities.clear();
        state.players.get_mut(P1).volonte = 2;
        assert!(!has_activate(&state));
    }

    // --- end turn ---

    #[test]
    fn end_turn_is_always_last() {
        let reg = registry_with(vec![character("A", 1, 2, 1, 3)]);
        let state = blank_state();
        let actions = get_valid_actions(&state, &reg, P1);
        assert_eq!(actions.last(), Some(&GameAction::EndTurn));
    }
}

//! Board queries and the KO removal — port of the query section and of
//! `removeFromBoard` in `src/engine/board.ts`.
//!
//! Every function takes the `CardRegistry` explicitly where the TS code calls
//! the global `getCardDef`; a missing definition is `Err(UnknownCard)` where
//! TS would `throw`.
//!
//! // PORT: the remaining board.ts mutations (`deployCharacter`, `equipObject`
//! // — needs fruits.ts —, `deployShip`, `moveCharacter`) and `getValidTargets`
//! // are ported with the action module (turnManager.ts).

use crate::error::EngineError;
use crate::registry::CardRegistry;
use crate::state::{CardInstance, GameState};
use crate::types::{
    AllyFilter, CardDef, CardType, Faction, ModifierStat, PassiveEffect, PlayerId, Slot, Trait,
    Zone,
};

// ============================================================
// Queries
// ============================================================

/// TS `getBoardCharacters(state, playerId)` — the occupants of `ALL_SLOTS`
/// in slot order (ids whose instance is missing are skipped).
pub fn get_board_characters(state: &GameState, player_id: PlayerId) -> Vec<&CardInstance> {
    let player = state.players.get(player_id);
    let mut result = Vec::new();
    for slot in Slot::ALL {
        if let Some(id) = player.board.get(slot) {
            if let Some(card) = state.cards.get(id) {
                result.push(card);
            }
        }
    }
    result
}

/// TS `getCharacterInSlot(state, playerId, slot)`.
pub fn get_character_in_slot(
    state: &GameState,
    player_id: PlayerId,
    slot: Slot,
) -> Option<&CardInstance> {
    state
        .players
        .get(player_id)
        .board
        .get(slot)
        .and_then(|id| state.cards.get(id))
}

/// TS `getEmptySlots(state, playerId)`.
pub fn get_empty_slots(state: &GameState, player_id: PlayerId) -> Vec<Slot> {
    state.players.get(player_id).board.empty_slots()
}

/// TS `getSlotOf(state, instanceId)` — `None` unless the card is on the board.
pub fn get_slot_of(state: &GameState, instance_id: &str) -> Option<Slot> {
    let card = state.cards.get(instance_id)?;
    if card.zone != Zone::Board {
        return None;
    }
    card.slot
}

/// TS `isFrontSlot(slot)`.
pub fn is_front_slot(slot: Slot) -> bool {
    Slot::FRONT.contains(&slot)
}

/// TS `isBackSlot(slot)`.
pub fn is_back_slot(slot: Slot) -> bool {
    Slot::BACK.contains(&slot)
}

/// TS `getAdjacentSlots(slot)`.
pub fn get_adjacent_slots(slot: Slot) -> &'static [Slot] {
    slot.adjacency()
}

/// TS `hasFrontRow(state, playerId)` — any front-row character, or the
/// flipped captain standing in a front slot.
pub fn has_front_row(state: &GameState, player_id: PlayerId) -> bool {
    let player = state.players.get(player_id);
    let has_char_in_front = Slot::FRONT.iter().any(|s| player.board.is_occupied(*s));
    if player.captain.flipped && player.captain.slot.is_some_and(is_front_slot) {
        return true;
    }
    has_char_in_front
}

/// TS `getEffectiveAtk(state, instanceId)` — base + equipment `bonusAtk` +
/// `atk` modifiers, floored at 0. `0` for an unknown instance.
pub fn get_effective_atk(
    state: &GameState,
    registry: &CardRegistry,
    instance_id: &str,
) -> Result<i32, EngineError> {
    let Some(card) = state.cards.get(instance_id) else {
        return Ok(0);
    };
    let def = registry.get_card_def(&card.def_id)?;
    let mut atk = def.atk.unwrap_or(0);

    // Equipment bonuses
    for obj_id in &card.attached_objects {
        if let Some(obj_card) = state.cards.get(obj_id) {
            let obj_def = registry.get_card_def(&obj_card.def_id)?;
            atk += obj_def.bonus_atk.unwrap_or(0);
        }
    }

    // Modifier bonuses
    for m in &card.modifiers {
        if m.stat == ModifierStat::Atk {
            atk += m.amount;
        }
    }

    Ok(atk.max(0))
}

/// TS `getEffectiveDef(state, instanceId)`.
pub fn get_effective_def(
    state: &GameState,
    registry: &CardRegistry,
    instance_id: &str,
) -> Result<i32, EngineError> {
    let Some(card) = state.cards.get(instance_id) else {
        return Ok(0);
    };
    let def = registry.get_card_def(&card.def_id)?;
    let mut def_val = def.def.unwrap_or(0);

    for obj_id in &card.attached_objects {
        if let Some(obj_card) = state.cards.get(obj_id) {
            let obj_def = registry.get_card_def(&obj_card.def_id)?;
            def_val += obj_def.bonus_def.unwrap_or(0);
        }
    }

    for m in &card.modifiers {
        if m.stat == ModifierStat::Def {
            def_val += m.amount;
        }
    }

    Ok(def_val.max(0))
}

/// TS `hasTrait(state, instanceId, trait)` — own traits plus traits granted by
/// equipped Devil Fruits (base, and awakening once `isAwakened`).
pub fn has_trait(
    state: &GameState,
    registry: &CardRegistry,
    instance_id: &str,
    t: Trait,
) -> Result<bool, EngineError> {
    let Some(card) = state.cards.get(instance_id) else {
        return Ok(false);
    };
    let def = registry.get_card_def(&card.def_id)?;
    if def.has_trait(t) {
        return Ok(true);
    }

    for obj_id in &card.attached_objects {
        let Some(obj_card) = state.cards.get(obj_id) else {
            continue;
        };
        let obj_def = registry.get_card_def(&obj_card.def_id)?;
        if let Some(fx) = &obj_def.fruit_effects {
            if fx
                .base
                .grants_traits
                .as_ref()
                .is_some_and(|ts| ts.contains(&t))
            {
                return Ok(true);
            }
            if obj_card.is_awakened.unwrap_or(false)
                && fx
                    .awakening
                    .as_ref()
                    .and_then(|a| a.grants_traits.as_ref())
                    .is_some_and(|ts| ts.contains(&t))
            {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// TS `hasSummoningSickness(state, instanceId)` — deployed this turn and no Rush.
pub fn has_summoning_sickness(
    state: &GameState,
    registry: &CardRegistry,
    instance_id: &str,
) -> Result<bool, EngineError> {
    let Some(card) = state.cards.get(instance_id) else {
        return Ok(false);
    };
    if card.deployed_turn == Some(state.turn_number) {
        return Ok(!has_trait(state, registry, instance_id, Trait::Rush)?);
    }
    Ok(false)
}

/// TS `deployCost(state, playerId, def)` — after `costReduction` passives and
/// ship reductions (min 1).
pub fn deploy_cost(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    def: &CardDef,
) -> Result<i32, EngineError> {
    let mut cost = def.cost;
    let player = state.players.get(player_id);
    let matches = |f: Option<&AllyFilter>| -> bool {
        let Some(f) = f else {
            return true;
        };
        if f.faction.is_some_and(|fa| def.faction != fa) {
            return false;
        }
        if f.tag.as_ref().is_some_and(|tag| !def.has_tag(tag)) {
            return false;
        }
        if f.trait_.is_some_and(|t| !def.has_trait(t)) {
            return false;
        }
        true
    };
    for slot in Slot::ALL {
        let Some(id) = player.board.get(slot) else {
            continue;
        };
        let d = registry.get_card_def(&state.get_card(id)?.def_id)?;
        for e in d.passive.as_ref().map(|p| p.effects.as_slice()).unwrap_or(&[]) {
            if let PassiveEffect::CostReduction { filter, amount } = e {
                if matches(filter.as_ref()) {
                    cost -= amount;
                }
            }
        }
    }
    if let Some(ship_id) = &player.active_ship {
        let sd = registry.get_card_def(&state.get_card(ship_id)?.def_id)?;
        let sp = sd.ship_passive.as_deref().unwrap_or("").to_lowercase();
        if (sp.contains("cout") || sp.contains("coût")) && sp.contains("-1") {
            let faction_ok = (sp.contains("marine") && def.faction == Faction::Marine)
                || (sp.contains("mugiwara") && def.has_tag("mugiwara"))
                || (!sp.contains("marine") && !sp.contains("mugiwara"));
            if faction_ok {
                cost -= 1;
            }
        }
    }
    Ok(cost.max(1))
}

// ============================================================
// Mutations
// ============================================================

/// TS `removeFromBoard(state, instanceId)` — a KO: frees the slot, resolves
/// the Vivre Card tutor, sends attached objects then the character to the
/// graveyard. No-op when the instance is missing or not on the board.
pub fn remove_from_board(
    state: &mut GameState,
    registry: &CardRegistry,
    instance_id: &str,
) -> Result<(), EngineError> {
    let Some(card) = state.cards.get(instance_id) else {
        return Ok(());
    };
    if card.zone != Zone::Board {
        return Ok(());
    }
    let owner = card.owner;
    let slot = card.slot;
    let attached: Vec<String> = card.attached_objects.clone();

    if let Some(slot) = slot {
        state.players.get_mut(owner).board.set(slot, None);
    }

    // Vivre Card: if the KO'd bearer held one, tutor a Mugiwara (cost <= 3) to hand.
    let had_vivre = attached
        .iter()
        .any(|id| state.cards.get(id).is_some_and(|c| c.def_id == "MG-019"));
    if had_vivre {
        let mut idx: Option<usize> = None;
        for (i, id) in state.players.get(owner).deck.iter().enumerate() {
            let d = registry.get_card_def(&state.get_card(id)?.def_id)?;
            if d.card_type == CardType::Character && d.has_tag("mugiwara") && d.cost <= 3 {
                idx = Some(i);
                break;
            }
        }
        if let Some(idx) = idx {
            let tutored = state.players.get_mut(owner).deck.remove(idx);
            let tutored_card = state.get_card_mut(&tutored)?;
            tutored_card.zone = Zone::Hand;
            let name = registry.get_card_def(&tutored_card.def_id)?.name.clone();
            state.players.get_mut(owner).hand.push(tutored);
            state.add_log(owner, format!("Vivre Card : {name} rejoint la main."));
        }
    }

    // Move attached objects to graveyard
    for obj_id in &attached {
        if let Some(obj) = state.cards.get_mut(obj_id) {
            obj.zone = Zone::Graveyard;
            state.players.get_mut(owner).graveyard.push(obj_id.clone());
        }
    }

    // Move character to graveyard
    let c = state.get_card_mut(instance_id)?;
    c.attached_objects = Vec::new();
    c.zone = Zone::Graveyard;
    c.slot = None;
    state
        .players
        .get_mut(owner)
        .graveyard
        .push(instance_id.to_string());
    Ok(())
}

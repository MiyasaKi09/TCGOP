//! Board queries and the KO removal — port of the query section and of
//! `removeFromBoard` in `src/engine/board.ts`.
//!
//! Every function takes the `CardRegistry` explicitly where the TS code calls
//! the global `getCardDef`; a missing definition is `Err(UnknownCard)` where
//! TS would `throw`.
//!
//! The mutations (`deployCharacter`, `equipObject`, `deployShip`,
//! `moveCharacter`) and `getValidTargets` live at the bottom of this file.
#![allow(clippy::collapsible_if)]
// ^ The nested `if` / `if let` blocks in this module mirror the TypeScript
// source branch for branch (see the per-function `PORT:` references). Merging
// them into let-chains would break that 1:1 reading, which is the whole point
// of the port, so the lint is turned off for this file only.

use serde::{Deserialize, Serialize};

use crate::context::EngineContext;
use crate::error::EngineError;
use crate::fruits::apply_fruit_base_effects;
use crate::passives::{apply_enemy_debuff_auras, recalculate_passive_buffs};
use crate::registry::CardRegistry;
use crate::state::{CardInstance, GameState};
use crate::types::{
    AllyFilter, AttackTrait, CardDef, CardType, Faction, Modifier, ModifierDuration, ModifierStat,
    ObjectSubtype, PassiveEffect, PlayerId, Slot, StatusEffectType, Trait, Zone,
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
        for e in d
            .passive
            .as_ref()
            .map(|p| p.effects.as_slice())
            .unwrap_or(&[])
        {
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

// ============================================================
// Mutations — ported from board.ts
// ============================================================

/// JS `sp.match(/\+(\d+)\s*<unit>/)` on an already lower-cased ship passive
/// (`unit` is `"pv"` or `"def"`): the first `+N` immediately followed by
/// optional whitespace and `unit`, as `parseInt` would read it.
fn match_ship_bonus(sp: &str, unit: &str) -> Option<i32> {
    let bytes = sp.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] != b'+' {
            continue;
        }
        let digits_start = i + 1;
        let mut j = digits_start;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j == digits_start {
            continue;
        }
        let mut k = j;
        while k < sp.len() {
            let rest = &sp[k..];
            if let Some(c) = rest.chars().next() {
                if is_js_space(c) {
                    k += c.len_utf8();
                    continue;
                }
            }
            break;
        }
        if sp[k..].starts_with(unit) {
            return Some(js_parse_int(&sp[digits_start..j]));
        }
    }
    None
}

/// JS `\s` — `WhiteSpace ∪ LineTerminator` exactly as ECMAScript defines it.
///
/// This is *not* Rust's `char::is_whitespace` (Unicode `White_Space`): JS
/// excludes U+0085 (NEL, which `White_Space` includes) and includes U+FEFF
/// (ZWNBSP, which `White_Space` excludes).
pub(crate) fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'      // <TAB>
        | '\u{000A}'    // <LF>
        | '\u{000B}'    // <VT>
        | '\u{000C}'    // <FF>
        | '\u{000D}'    // <CR>
        | '\u{0020}'    // <SP>
        | '\u{00A0}'    // <NBSP>
        | '\u{1680}'
        | '\u{2000}'
            ..='\u{200A}'
        | '\u{2028}'    // <LS>
        | '\u{2029}'    // <PS>
        | '\u{202F}'
        | '\u{205F}'
        | '\u{3000}'
        | '\u{FEFF}' // <ZWNBSP>
    )
}

/// JS `parseInt(digits, 10)` on a run of ASCII digits, narrowed to `i32`.
///
/// `parseInt` returns a double, so a digit run wider than `i32` still yields a
/// finite (large) number rather than failing. Parsing through `f64` reproduces
/// that; the `as i32` cast saturates, which is the closest `i32` can come to
/// the JS value (and, like TS, keeps the bonus non-zero).
pub(crate) fn js_parse_int(digits: &str) -> i32 {
    match digits.parse::<i32>() {
        Ok(v) => v,
        // Only ASCII digits reach here, so the `f64` parse cannot fail.
        Err(_) => digits.parse::<f64>().unwrap_or(0.0) as i32,
    }
}

/// The TS literal behind an [`ObjectSubtype`] (`"weapon"` / `"fruit"` / `"accessory"`).
fn subtype_str(subtype: ObjectSubtype) -> &'static str {
    match subtype {
        ObjectSubtype::Weapon => "weapon",
        ObjectSubtype::Fruit => "fruit",
        ObjectSubtype::Accessory => "accessory",
    }
}

/// `def.passive?.effects.some(pred)`.
fn passive_has(def: &CardDef, pred: impl Fn(&PassiveEffect) -> bool) -> bool {
    def.passive
        .as_ref()
        .is_some_and(|p| p.effects.iter().any(pred))
}

/// TS `deployCharacter(state, playerId, instanceId, slot)`
/// — `src/engine/board.ts:228`.
///
/// Validates ownership / zone / type / free slot, pays [`deploy_cost`], moves the
/// card from hand to `slot` (`currentPv = def.pv ?? 0`, `deployedTurn = turnNumber`),
/// applies the active ship's `+N PV` / `+N DEF` deploy bonus parsed out of
/// `shipPassive`, logs `"Deploie {name} en {slot}"`, then runs the `copyAtkOnDeploy`
/// (Mr. 2) and `entryDiscardRandom` (Robin) entry passives and finally
/// `recalculate_passive_buffs(playerId)` + `apply_enemy_debuff_auras`.
///
/// Errors mirror the TS throws: `Card not found: {id}`, `Not your card`,
/// `Card not in hand`, `Not a character card`, `Slot {slot} is occupied`,
/// `Cannot afford {name} (cost {cost})`.
pub fn deploy_character(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    instance_id: &str,
    slot: Slot,
) -> Result<(), EngineError> {
    let Some(card) = state.cards.get(instance_id) else {
        return Err(EngineError::illegal(format!(
            "Card not found: {instance_id}"
        )));
    };
    if card.owner != player_id {
        return Err(EngineError::illegal("Not your card"));
    }
    if card.zone != Zone::Hand {
        return Err(EngineError::illegal("Card not in hand"));
    }

    let def = registry.get_card_def(&card.def_id)?.clone();
    if def.card_type != CardType::Character {
        return Err(EngineError::illegal("Not a character card"));
    }

    if state.players.get(player_id).board.is_occupied(slot) {
        return Err(EngineError::SlotOccupied(slot));
    }

    let cost = deploy_cost(state, registry, player_id, &def)?;
    if !state.can_afford(player_id, cost) {
        return Err(EngineError::CannotAfford {
            what: def.name.clone(),
            cost,
            has: state.players.get(player_id).volonte,
        });
    }

    state.spend_volonte(player_id, cost)?;

    let turn_number = state.turn_number;
    {
        let p = state.players.get_mut(player_id);
        // Remove from hand
        p.hand.retain(|id| id != instance_id);
        // Place on board
        p.board.set(slot, Some(instance_id.to_string()));
    }
    {
        let c = state.get_card_mut(instance_id)?;
        c.zone = Zone::Board;
        c.slot = Some(slot);
        c.deployed_turn = Some(turn_number);
        c.current_pv = def.pv.unwrap_or(0);
    }

    // At-deploy ship buffs (Going Merry +1 PV, Navire de Guerre +1 DEF, etc.).
    if let Some(ship_id) = state.players.get(player_id).active_ship.clone() {
        let sd = registry.get_card_def(&state.get_card(&ship_id)?.def_id)?;
        let sd_id = sd.id.clone();
        let sp = sd.ship_passive.as_deref().unwrap_or("").to_lowercase();
        if sp.contains("deploiement") || sp.contains("déploiement") {
            let faction_ok = (sp.contains("mugiwara") && def.has_tag("mugiwara"))
                || (sp.contains("marine") && def.faction == Faction::Marine)
                || (!sp.contains("mugiwara") && !sp.contains("marine"));
            if faction_ok {
                let pv = match_ship_bonus(&sp, "pv");
                let dfb = match_ship_bonus(&sp, "def");
                let c = state.get_card_mut(instance_id)?;
                if let Some(amount) = pv {
                    c.current_pv += amount;
                    c.modifiers.push(Modifier {
                        id: format!("shipdep_pv_{instance_id}"),
                        stat: ModifierStat::Pv,
                        amount,
                        source: format!("ship_{sd_id}"),
                        duration: ModifierDuration::Permanent,
                        turns_remaining: None,
                    });
                }
                if let Some(amount) = dfb {
                    c.modifiers.push(Modifier {
                        id: format!("shipdep_def_{instance_id}"),
                        stat: ModifierStat::Def,
                        amount,
                        source: format!("ship_{sd_id}"),
                        duration: ModifierDuration::Permanent,
                        turns_remaining: None,
                    });
                }
            }
        }
    }

    state.add_log(
        player_id,
        format!("Deploie {} en {}", def.name, slot.as_str()),
    );

    // Mr. 2 (Bon Clay): copy the ATK of one of your other characters on deploy.
    if passive_has(&def, |e| matches!(e, PassiveEffect::CopyAtkOnDeploy)) {
        let mut best_atk = def.atk.unwrap_or(0);
        for s in Slot::ALL {
            let Some(oid) = state.players.get(player_id).board.get(s).cloned() else {
                continue;
            };
            if oid == instance_id {
                continue;
            }
            best_atk = best_atk.max(get_effective_atk(state, registry, &oid)?);
        }
        let delta = best_atk - def.atk.unwrap_or(0);
        if delta > 0 {
            state.get_card_mut(instance_id)?.modifiers.push(Modifier {
                id: format!("manemane_{instance_id}"),
                stat: ModifierStat::Atk,
                amount: delta,
                source: format!("passive_{instance_id}"),
                duration: ModifierDuration::Permanent,
                turns_remaining: None,
            });
        }
    }

    // Miss All Sunday (Robin): opponent discards a random card on entry.
    if passive_has(&def, |e| matches!(e, PassiveEffect::EntryDiscardRandom)) {
        let opp = player_id.opponent();
        let hand_len = state.players.get(opp).hand.len();
        if hand_len > 0 {
            let i = ctx.rng.random_index(hand_len);
            let disc = state.players.get_mut(opp).hand.remove(i);
            state.get_card_mut(&disc)?.zone = Zone::Graveyard;
            state.players.get_mut(opp).graveyard.push(disc);
            state.add_log(
                player_id,
                format!("{} : l'adversaire défausse une carte.", def.name),
            );
        }
    }

    // Recalculate passive buffs (new character on board may trigger synergies, captain buffs)
    recalculate_passive_buffs(state, registry, player_id)?;
    apply_enemy_debuff_auras(state, registry)?;

    Ok(())
}

/// TS `equipObject(state, playerId, objectInstanceId, targetInstanceId)`
/// — `src/engine/board.ts:329`.
///
/// Validates the object (hand, owned, `type === "object"`), computes the
/// Clima-Tact (`MG-012`) 0-cost combo, enforces the weapon/fruit/accessory slot
/// caps (`threeWeaponSlots` > `twoWeaponSlots`, `twoAccessorySlots`), pays, attaches,
/// grants the four signature-weapon wielder bonuses (MG-009/MR-013/RH-011/RH-013),
/// logs `"Equipe {obj} sur {target}"`, then calls
/// [`crate::fruits::apply_fruit_base_effects`] for fruits and
/// `recalculate_passive_buffs(playerId)`.
///
/// Errors: `Object not found: {id}`, `Not your card`, `Object not in hand`,
/// `Not an object card`, `Target not found: {id}`, `Not your character`,
/// `Target not on board`, `Cannot afford {name} (cost {cost})`,
/// `{targetName} already has max {subtype} equipped`.
pub fn equip_object(
    state: &mut GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    object_instance_id: &str,
    target_instance_id: &str,
) -> Result<(), EngineError> {
    let Some(obj_card) = state.cards.get(object_instance_id) else {
        return Err(EngineError::illegal(format!(
            "Object not found: {object_instance_id}"
        )));
    };
    if obj_card.owner != player_id {
        return Err(EngineError::illegal("Not your card"));
    }
    if obj_card.zone != Zone::Hand {
        return Err(EngineError::illegal("Object not in hand"));
    }

    let obj_def = registry.get_card_def(&obj_card.def_id)?.clone();
    if obj_def.card_type != CardType::Object {
        return Err(EngineError::illegal("Not an object card"));
    }

    let Some(target_card) = state.cards.get(target_instance_id) else {
        return Err(EngineError::illegal(format!(
            "Target not found: {target_instance_id}"
        )));
    };
    if target_card.owner != player_id {
        return Err(EngineError::illegal("Not your character"));
    }
    if target_card.zone != Zone::Board {
        return Err(EngineError::illegal("Target not on board"));
    }

    // Clima-Tact combo: costs 0 if both Usopp (MG-004) and Nami (MG-003) are in play.
    let mut effective_cost = obj_def.cost;
    if obj_def.id == "MG-012" {
        let ids: Vec<String> = get_board_characters(state, player_id)
            .into_iter()
            .map(|c| c.def_id.clone())
            .collect();
        if ids.iter().any(|d| d == "MG-003") && ids.iter().any(|d| d == "MG-004") {
            effective_cost = 0;
        }
    }
    if !state.can_afford(player_id, effective_cost) {
        return Err(EngineError::CannotAfford {
            what: obj_def.name.clone(),
            cost: effective_cost,
            has: state.players.get(player_id).volonte,
        });
    }

    // Check equipment slot limits (simplified — 1 weapon, 1 fruit, 1 accessory)
    let target_def = registry
        .get_card_def(&state.get_card(target_instance_id)?.def_id)?
        .clone();
    let attached: Vec<String> = state.get_card(target_instance_id)?.attached_objects.clone();
    let mut existing_subtypes: Vec<Option<ObjectSubtype>> = Vec::with_capacity(attached.len());
    for id in &attached {
        let d = registry.get_card_def(&state.get_card(id)?.def_id)?;
        existing_subtypes.push(d.subtype);
    }

    if let Some(subtype) = obj_def.subtype {
        let same_subtype = existing_subtypes
            .iter()
            .filter(|d| **d == Some(subtype))
            .count();
        // Check for exceptions (Zoro 3 weapons, Franky 2 accessories)
        let mut max_slots = 1usize;
        let passive_effects: &[PassiveEffect] = target_def
            .passive
            .as_ref()
            .map(|p| p.effects.as_slice())
            .unwrap_or(&[]);
        if subtype == ObjectSubtype::Weapon {
            if passive_effects
                .iter()
                .any(|e| matches!(e, PassiveEffect::ThreeWeaponSlots))
            {
                max_slots = 3;
            } else if passive_effects
                .iter()
                .any(|e| matches!(e, PassiveEffect::TwoWeaponSlots))
            {
                max_slots = 2;
            }
        }
        if subtype == ObjectSubtype::Accessory
            && passive_effects
                .iter()
                .any(|e| matches!(e, PassiveEffect::TwoAccessorySlots))
        {
            max_slots = 2;
        }
        if same_subtype >= max_slots {
            return Err(EngineError::illegal(format!(
                "{} already has max {} equipped",
                target_def.name,
                subtype_str(subtype)
            )));
        }
    }

    state.spend_volonte(player_id, effective_cost)?;

    let target_slot = state.get_card(target_instance_id)?.slot;
    {
        // Remove from hand
        state
            .players
            .get_mut(player_id)
            .hand
            .retain(|id| id != object_instance_id);
    }
    {
        // Attach to character
        let obj = state.get_card_mut(object_instance_id)?;
        obj.zone = Zone::Board;
        obj.slot = target_slot;
    }
    state
        .get_card_mut(target_instance_id)?
        .attached_objects
        .push(object_instance_id.to_string());

    // Signature-weapon bonuses when wielded by the matching character.
    let wielder_bonus: Option<(&str, ModifierStat, i32)> = match obj_def.id.as_str() {
        "MG-009" => Some(("Zoro", ModifierStat::Def, 1)), // Wado Ichimonji
        "MR-013" => Some(("Tashigi", ModifierStat::Atk, 1)), // Shigure
        "RH-011" => Some(("Ben Beckman", ModifierStat::Atk, 1)), // Fusil de Beckman
        "RH-013" => Some(("Yasopp", ModifierStat::Atk, 1)), // Fusil de Yasopp
        _ => None,
    };
    if let Some((name, stat, amount)) = wielder_bonus {
        if target_def.name.contains(name) {
            state
                .get_card_mut(target_instance_id)?
                .modifiers
                .push(Modifier {
                    id: format!("wield_{object_instance_id}"),
                    stat,
                    amount,
                    source: format!("equip_{}", obj_def.id),
                    duration: ModifierDuration::Permanent,
                    turns_remaining: None,
                });
        }
    }

    state.add_log(
        player_id,
        format!("Equipe {} sur {}", obj_def.name, target_def.name),
    );

    // Apply Devil Fruit effects if it's a fruit
    if obj_def.subtype == Some(ObjectSubtype::Fruit) && obj_def.fruit_effects.is_some() {
        apply_fruit_base_effects(state, registry, object_instance_id, target_instance_id)?;
    }

    // Recalculate passive buffs
    recalculate_passive_buffs(state, registry, player_id)?;

    Ok(())
}

/// TS `deployShip(state, playerId, instanceId)` — `src/engine/board.ts:440`.
///
/// Pays `def.cost`, resolves the previous ship's `shipDestroyEffect`
/// (`healAll` / `draw` / `deployToken`, log `"{oldName} : effet de destruction."`)
/// and sends it to the graveyard, then sets `activeShip` and logs
/// `"Deploie navire {name}"`.
///
/// Errors: `Card not found: {id}`, `Not your card`, `Card not in hand`,
/// `Not a ship card`, `Cannot afford {name} (cost {cost})`.
pub fn deploy_ship(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    instance_id: &str,
) -> Result<(), EngineError> {
    let Some(card) = state.cards.get(instance_id) else {
        return Err(EngineError::illegal(format!(
            "Card not found: {instance_id}"
        )));
    };
    if card.owner != player_id {
        return Err(EngineError::illegal("Not your card"));
    }
    if card.zone != Zone::Hand {
        return Err(EngineError::illegal("Card not in hand"));
    }

    let def = registry.get_card_def(&card.def_id)?.clone();
    if def.card_type != CardType::Ship {
        return Err(EngineError::illegal("Not a ship card"));
    }

    if !state.can_afford(player_id, def.cost) {
        return Err(EngineError::CannotAfford {
            what: def.name.clone(),
            cost: def.cost,
            has: state.players.get(player_id).volonte,
        });
    }

    state.spend_volonte(player_id, def.cost)?;

    // Discard previous ship if any — may trigger its destruction effect (Going Merry).
    if let Some(active_ship) = state.players.get(player_id).active_ship.clone() {
        if state.cards.contains_key(&active_ship) {
            let old_def = registry
                .get_card_def(&state.get_card(&active_ship)?.def_id)?
                .clone();
            if let Some(de) = old_def.ship_destroy_effect.clone() {
                // TS `if (de.healAll)` — JS truthiness, so `0` is skipped.
                if let Some(heal_all) = de.heal_all.filter(|v| *v != 0) {
                    for s in Slot::ALL {
                        let Some(cid) = state.players.get(player_id).board.get(s).cloned() else {
                            continue;
                        };
                        if let Some(c) = state.cards.get(&cid) {
                            let current = c.current_pv;
                            let cap = registry.get_card_def(&c.def_id)?.pv.unwrap_or(current);
                            state.get_card_mut(&cid)?.current_pv = (current + heal_all).min(cap);
                        }
                    }
                }
                if let Some(draw) = de.draw {
                    if draw != 0 && !state.players.get(player_id).deck.is_empty() {
                        let mut i = 0;
                        while i < draw && !state.players.get(player_id).deck.is_empty() {
                            let id = state.players.get_mut(player_id).deck.remove(0);
                            state.get_card_mut(&id)?.zone = Zone::Hand;
                            state.players.get_mut(player_id).hand.push(id);
                            i += 1;
                        }
                    }
                }
                // TS `if (de.deployToken)` — JS truthiness, so `""` is skipped
                // (and never reaches `getCardDef("")`).
                if let Some(token_def_id) = de.deploy_token.as_ref().filter(|t| !t.is_empty()) {
                    let empty = Slot::ALL
                        .into_iter()
                        .find(|s| !state.players.get(player_id).board.is_occupied(*s));
                    if let Some(empty) = empty {
                        let tdef = registry.get_card_def(token_def_id)?;
                        let token_pv = tdef.pv.unwrap_or(1);
                        let tid = ctx.generate_instance_id(token_def_id);
                        let mut token = CardInstance::new(
                            tid.clone(),
                            token_def_id.clone(),
                            player_id,
                            token_pv,
                        );
                        token.zone = Zone::Board;
                        token.slot = Some(empty);
                        token.deployed_turn = Some(state.turn_number);
                        state.add_instance(token);
                        state.players.get_mut(player_id).board.set(empty, Some(tid));
                    }
                }
                state.add_log(
                    player_id,
                    format!("{} : effet de destruction.", old_def.name),
                );
            }
            state.get_card_mut(&active_ship)?.zone = Zone::Graveyard;
            state
                .players
                .get_mut(player_id)
                .graveyard
                .push(active_ship.clone());
        }
    }

    {
        let p = state.players.get_mut(player_id);
        // Remove from hand
        p.hand.retain(|id| id != instance_id);
        // Set as active ship
        p.active_ship = Some(instance_id.to_string());
    }
    state.get_card_mut(instance_id)?.zone = Zone::Board;

    state.add_log(player_id, format!("Deploie navire {}", def.name));
    Ok(())
}

/// TS `moveCharacter(state, playerId, instanceId, targetSlot)`
/// — `src/engine/board.ts:519`.
///
/// The free 1x/turn reposition to an adjacent empty slot; sets `usedFreeMove`.
/// No log line in the TS source.
///
/// Errors: `Free move already used this turn`, `Card not on board`,
/// `Not your card`, `Card has no slot`, `{targetSlot} is not adjacent to
/// {currentSlot}`, `Slot {targetSlot} is occupied`.
pub fn move_character(
    state: &mut GameState,
    player_id: PlayerId,
    instance_id: &str,
    target_slot: Slot,
) -> Result<(), EngineError> {
    if state.players.get(player_id).used_free_move {
        return Err(EngineError::illegal("Free move already used this turn"));
    }

    let card = state.cards.get(instance_id);
    if !card.is_some_and(|c| c.zone == Zone::Board) {
        return Err(EngineError::illegal("Card not on board"));
    }
    let card = state.get_card(instance_id)?;
    if card.owner != player_id {
        return Err(EngineError::illegal("Not your card"));
    }

    let Some(current_slot) = card.slot else {
        return Err(EngineError::illegal("Card has no slot"));
    };

    if !get_adjacent_slots(current_slot).contains(&target_slot) {
        return Err(EngineError::NotAdjacent {
            from: current_slot,
            to: target_slot,
        });
    }

    if state.players.get(player_id).board.is_occupied(target_slot) {
        return Err(EngineError::SlotOccupied(target_slot));
    }

    {
        let p = state.players.get_mut(player_id);
        p.board.set(current_slot, None);
        p.board.set(target_slot, Some(instance_id.to_string()));
        p.used_free_move = true;
    }
    state.get_card_mut(instance_id)?.slot = Some(target_slot);

    Ok(())
}

// ============================================================
// Valid targets for attacks
// ============================================================

/// TS return type of `getValidTargets` —
/// `{ characterTargets: string[]; canTargetCaptain: boolean }`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidTargets {
    /// Instance ids of the enemy characters this attacker may hit, in board order.
    pub character_targets: Vec<String>,
    /// Whether the enemy captain may be targeted by this attack.
    pub can_target_captain: bool,
}

/// One entry of [`ValidTargets`] — a single legal target, used by callers that
/// want the character targets and the captain in one ordered list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ValidTarget {
    /// An enemy character on the board (`targetIsCaptain` absent / false).
    Character { instance_id: String },
    /// The enemy captain (`targetInstanceId = "captain_{playerId}"`,
    /// `targetIsCaptain: true`).
    Captain { player_id: PlayerId },
}

/// TS `getValidTargets(state, attackerInstanceId, forSpecial?)`
/// — `src/engine/board.ts:623`.
///
/// Range is read from the attacker's own `range` trait or from the chosen
/// attack's `attackTraits`; a back-row attacker without Range gets nothing.
/// Without Range, a defender that `has_front_row` restricts targets to V1–V3.
/// Stealth units drop out while any non-Stealth (or `noStealth`-tagged) target
/// remains. A flipped captain is targetable like a character; a recto captain
/// only while the defender has zero board characters.
pub fn get_valid_targets(
    state: &GameState,
    registry: &CardRegistry,
    attacker_instance_id: &str,
    for_special: bool,
) -> Result<ValidTargets, EngineError> {
    let Some(attacker) = state.cards.get(attacker_instance_id) else {
        return Ok(ValidTargets::default());
    };

    let attacker_def = registry.get_card_def(&attacker.def_id)?;
    let Some(attacker_slot) = attacker.slot else {
        return Ok(ValidTargets::default());
    };

    let opponent_id = attacker.owner.opponent();

    // Check range from character trait OR from the specific attack's traits
    let mut has_range = attacker_def.has_trait(Trait::Range);
    if !has_range
        && for_special
        && attacker_def.special_attack.as_ref().is_some_and(|sa| {
            sa.attack_traits
                .as_ref()
                .is_some_and(|ts| ts.contains(&AttackTrait::Range))
        })
    {
        has_range = true;
    }
    if !has_range
        && !for_special
        && attacker_def.base_action.as_ref().is_some_and(|ba| {
            ba.attack_traits
                .as_ref()
                .is_some_and(|ts| ts.contains(&AttackTrait::Range))
        })
    {
        has_range = true;
    }

    let attacker_in_back = is_back_slot(attacker_slot);

    // Back row without Range can't melee attack
    if attacker_in_back && !has_range {
        return Ok(ValidTargets::default());
    }

    let opponent_has_front = has_front_row(state, opponent_id);
    let opponent_chars: Vec<String> = get_board_characters(state, opponent_id)
        .into_iter()
        .map(|c| c.instance_id.clone())
        .collect();

    // Determine targetable characters
    let mut targetable: Vec<String> = opponent_chars.clone();

    if opponent_has_front && !has_range {
        // Can only target front row
        targetable.retain(|id| {
            state
                .cards
                .get(id)
                .and_then(|c| c.slot)
                .is_some_and(is_front_slot)
        });
    }

    // Apply Stealth filter (a unit stripped of Furtif this turn counts as non-stealth)
    let mut stealthed: Vec<bool> = Vec::with_capacity(targetable.len());
    for id in &targetable {
        let s = has_trait(state, registry, id, Trait::Stealth)?
            && !state
                .cards
                .get(id)
                .is_some_and(|c| c.has_status(StatusEffectType::NoStealth));
        stealthed.push(s);
    }
    let has_non_stealth = stealthed.iter().any(|s| !*s);
    if has_non_stealth {
        targetable = targetable
            .into_iter()
            .zip(stealthed.iter())
            .filter(|(_, s)| !**s)
            .map(|(id, _)| id)
            .collect();
    }

    // Can target captain?
    let opponent = state.players.get(opponent_id);
    let mut can_target_captain = false;
    if opponent.captain.flipped && opponent.captain.slot.is_some() {
        // Verso captain is on board — targetable like a character
        // (subject to front row protection)
        // Two arms with the same body, exactly as in board.ts:681-685 — kept
        // apart so the two distinct rules (range/no-front vs. captain standing
        // in the front row) stay readable next to the TS source.
        #[allow(clippy::if_same_then_else)]
        if !opponent_has_front || has_range {
            can_target_captain = true;
        } else if opponent.captain.slot.is_some_and(is_front_slot) {
            can_target_captain = true;
        }
    } else {
        // Recto captain is off-board, but becomes EXPOSED when its owner has no characters
        // on the board — the crew is wiped (Rulebook v3.1 §2.1). Re-protected as soon as
        // any ally returns to the board.
        can_target_captain = opponent_chars.is_empty();
    }

    Ok(ValidTargets {
        character_targets: targetable,
        can_target_captain,
    })
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Board, CaptainInstance, PlayerState, Players};
    use crate::types::{
        BaseAction, CaptainDef, CaptainRecto, CaptainVerso, EntryEffect, FlipCondition, PassiveDef,
        Phase, Rarity, ShipDestroyEffect, SpecialAttack, StatusEffect,
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

    fn object(id: &str, cost: i32, subtype: ObjectSubtype) -> CardDef {
        let mut d = CardDef::new(
            id,
            format!("Obj {id}"),
            CardType::Object,
            cost,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        d.subtype = Some(subtype);
        d
    }

    fn ship(id: &str, cost: i32, passive: &str) -> CardDef {
        let mut d = CardDef::new(
            id,
            format!("Ship {id}"),
            CardType::Ship,
            cost,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        d.ship_passive = Some(passive.to_string());
        d
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

    /// Insert an instance in `zone`, returning its instance id (`<defId>#<n>`).
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

    // --- match_ship_bonus (the JS regexes) ---

    #[test]
    fn ship_bonus_regex_matches_the_js_pattern() {
        let sp = "vos mugiwara ont +1 pv au déploiement.";
        assert_eq!(match_ship_bonus(sp, "pv"), Some(1));
        assert_eq!(match_ship_bonus(sp, "def"), None);
        assert_eq!(match_ship_bonus("+2pv", "pv"), Some(2));
        assert_eq!(match_ship_bonus("+1 atk +3 def", "def"), Some(3));
        assert_eq!(match_ship_bonus("+1 2 pv", "pv"), None);
        assert_eq!(match_ship_bonus("pv +", "pv"), None);
    }

    #[test]
    fn ship_bonus_uses_the_js_whitespace_class_and_parse_int() {
        // JS `\s` includes U+FEFF (which Unicode White_Space does not) …
        assert_eq!(match_ship_bonus("+1\u{feff}pv", "pv"), Some(1));
        // … and excludes U+0085 NEL (which Unicode White_Space does include).
        assert_eq!(match_ship_bonus("+1\u{85}pv", "pv"), None);
        // The rest of the class behaves like `\s`.
        for sep in [
            "\u{9}", "\u{a}", "\u{b}", "\u{c}", "\u{d}", " ", "\u{a0}", "\u{1680}", "\u{2000}",
            "\u{200a}", "\u{2028}", "\u{2029}", "\u{202f}", "\u{205f}", "\u{3000}",
        ] {
            assert_eq!(
                match_ship_bonus(&format!("+2{sep}pv"), "pv"),
                Some(2),
                "{sep:?}"
            );
        }
        // `parseInt` returns a double, so a digit run wider than `i32` is still
        // a (large) finite bonus, never "no match".
        assert_eq!(match_ship_bonus("+99999999999 pv", "pv"), Some(i32::MAX));
        assert_eq!(match_ship_bonus("+2147483647 pv", "pv"), Some(i32::MAX));
    }

    // --- deployCost ---

    #[test]
    fn deploy_cost_stacks_passives_and_ship_and_floors_at_one() {
        let mut helper = character("HELP", 1, 1, 1, 1);
        helper.passive = Some(PassiveDef {
            name: "p".into(),
            description: "d".into(),
            effects: vec![PassiveEffect::CostReduction {
                filter: None,
                amount: 1,
            }],
        });
        let target = character("TGT", 4, 2, 2, 3);
        let reg = registry_with(vec![
            helper,
            target.clone(),
            ship("SHIP", 2, "Le cout de vos Mugiwara est reduit de -1."),
        ]);
        let mut state = blank_state();

        assert_eq!(deploy_cost(&state, &reg, P1, &target).unwrap(), 4);

        put(&mut state, &reg, "HELP", P1, Zone::Board, Some(Slot::V1));
        put(&mut state, &reg, "HELP", P1, Zone::Board, Some(Slot::V2));
        assert_eq!(deploy_cost(&state, &reg, P1, &target).unwrap(), 2);

        // Ship: "cout" + "-1", and neither "marine" nor "mugiwara"… the text
        // here mentions mugiwara, so the tag gate applies and the card has no tag.
        let sid = put(&mut state, &reg, "SHIP", P1, Zone::Board, None);
        state.players.get_mut(P1).active_ship = Some(sid);
        assert_eq!(deploy_cost(&state, &reg, P1, &target).unwrap(), 2);

        // A mugiwara-tagged card gets the extra -1, and the result floors at 1.
        let mut mugi = character("MUGI", 1, 1, 1, 1);
        mugi.tags = Some(vec!["mugiwara".into()]);
        assert_eq!(deploy_cost(&state, &reg, P1, &mugi).unwrap(), 1);
    }

    // --- deployCharacter ---

    #[test]
    fn deploy_character_moves_pays_logs_and_applies_ship_deploy_bonus() {
        let mut mugi = character("MG-001", 2, 2, 1, 3);
        mugi.tags = Some(vec!["mugiwara".into()]);
        let reg = registry_with(vec![
            mugi,
            ship("MG-020", 2, "Vos Mugiwara ont +1 PV au déploiement."),
        ]);
        let mut state = blank_state();
        let mut ctx = EngineContext::seeded(1);

        let sid = put(&mut state, &reg, "MG-020", P1, Zone::Board, None);
        state.players.get_mut(P1).active_ship = Some(sid);
        let cid = put(&mut state, &reg, "MG-001", P1, Zone::Hand, None);

        deploy_character(&mut state, &reg, &mut ctx, P1, &cid, Slot::V2).unwrap();

        assert_eq!(state.players.get(P1).volonte, 8);
        assert!(state.players.get(P1).hand.is_empty());
        assert_eq!(state.players.get(P1).board.get(Slot::V2), Some(&cid));
        let c = state.card(&cid).unwrap();
        assert_eq!(c.zone, Zone::Board);
        assert_eq!(c.slot, Some(Slot::V2));
        assert_eq!(c.deployed_turn, Some(3));
        assert_eq!(c.current_pv, 4); // 3 printed + 1 from the ship
        assert_eq!(c.modifiers.len(), 1);
        assert_eq!(c.modifiers[0].id, format!("shipdep_pv_{cid}"));
        assert_eq!(c.modifiers[0].stat, ModifierStat::Pv);
        assert_eq!(c.modifiers[0].source, "ship_MG-020");
        assert_eq!(state.log[0].message, "Deploie Char MG-001 en V2");
    }

    #[test]
    fn deploy_character_rejects_occupied_slot_and_unaffordable_cost() {
        let reg = registry_with(vec![
            character("A", 12, 1, 1, 1),
            character("B", 1, 1, 1, 1),
        ]);
        let mut state = blank_state();
        let mut ctx = EngineContext::seeded(1);

        put(&mut state, &reg, "B", P1, Zone::Board, Some(Slot::V1));
        let a = put(&mut state, &reg, "A", P1, Zone::Hand, None);
        assert_eq!(
            deploy_character(&mut state, &reg, &mut ctx, P1, &a, Slot::V1),
            Err(EngineError::SlotOccupied(Slot::V1))
        );
        assert_eq!(
            deploy_character(&mut state, &reg, &mut ctx, P1, &a, Slot::V2),
            Err(EngineError::CannotAfford {
                what: "Char A".into(),
                cost: 12,
                has: 10
            })
        );
        // Wrong owner / wrong zone.
        assert_eq!(
            deploy_character(&mut state, &reg, &mut ctx, P2, &a, Slot::V2),
            Err(EngineError::illegal("Not your card"))
        );
    }

    #[test]
    fn deploy_character_copy_atk_on_deploy_and_entry_discard_random() {
        let mut bonclay = character("MR2", 1, 1, 1, 3);
        bonclay.passive = Some(PassiveDef {
            name: "p".into(),
            description: "d".into(),
            effects: vec![
                PassiveEffect::CopyAtkOnDeploy,
                PassiveEffect::EntryDiscardRandom,
            ],
        });
        let reg = registry_with(vec![bonclay, character("BIG", 1, 7, 1, 3)]);
        let mut state = blank_state();
        let mut ctx = EngineContext::seeded(42);

        put(&mut state, &reg, "BIG", P1, Zone::Board, Some(Slot::V1));
        let opp_card = put(&mut state, &reg, "BIG", P2, Zone::Hand, None);
        let mr2 = put(&mut state, &reg, "MR2", P1, Zone::Hand, None);

        deploy_character(&mut state, &reg, &mut ctx, P1, &mr2, Slot::V2).unwrap();

        // TS quirk, reproduced verbatim: `copyAtkOnDeploy` pushes
        // `manemane_<id>` with `source: passive_<id>`, and the
        // `recalculatePassiveBuffs` call at the end of `deployCharacter`
        // strips every `passive_*`-sourced modifier again — so the Mr. 2 copy
        // never survives its own deploy.
        assert!(
            state
                .card(&mr2)
                .unwrap()
                .modifiers
                .iter()
                .all(|m| m.id != format!("manemane_{mr2}"))
        );
        assert_eq!(get_effective_atk(&state, &reg, &mr2).unwrap(), 1);

        // Robin: the (only) opponent hand card is discarded.
        assert!(state.players.get(P2).hand.is_empty());
        assert_eq!(state.players.get(P2).graveyard, vec![opp_card.clone()]);
        assert_eq!(state.card(&opp_card).unwrap().zone, Zone::Graveyard);
        assert_eq!(
            state.log.last().unwrap().message,
            "Char MR2 : l'adversaire défausse une carte."
        );
        assert_eq!(state.log.last().unwrap().player, P1);
    }

    // --- equipObject ---

    #[test]
    fn equip_object_enforces_slot_caps_and_grants_the_wielder_bonus() {
        let mut zoro = character("MG-002", 3, 3, 2, 4);
        zoro.name = "Roronoa Zoro".into();
        zoro.passive = Some(PassiveDef {
            name: "p".into(),
            description: "d".into(),
            effects: vec![PassiveEffect::ThreeWeaponSlots],
        });
        let plain = character("MG-005", 2, 1, 1, 3);
        let reg = registry_with(vec![
            zoro,
            plain,
            object("MG-009", 1, ObjectSubtype::Weapon),
            object("MG-010", 1, ObjectSubtype::Weapon),
        ]);
        let mut state = blank_state();

        let z = put(&mut state, &reg, "MG-002", P1, Zone::Board, Some(Slot::V1));
        let w1 = put(&mut state, &reg, "MG-009", P1, Zone::Hand, None);
        equip_object(&mut state, &reg, P1, &w1, &z).unwrap();

        let zc = state.card(&z).unwrap();
        assert_eq!(zc.attached_objects, vec![w1.clone()]);
        assert_eq!(state.card(&w1).unwrap().zone, Zone::Board);
        assert_eq!(state.card(&w1).unwrap().slot, Some(Slot::V1));
        let m = zc
            .modifiers
            .iter()
            .find(|m| m.id == format!("wield_{w1}"))
            .unwrap();
        assert_eq!((m.stat, m.amount), (ModifierStat::Def, 1));
        assert_eq!(m.source, "equip_MG-009");
        assert_eq!(state.log[0].message, "Equipe Obj MG-009 sur Roronoa Zoro");
        assert_eq!(state.players.get(P1).volonte, 9);

        // A plain character only has one weapon slot.
        let p = put(&mut state, &reg, "MG-005", P1, Zone::Board, Some(Slot::V2));
        let w2 = put(&mut state, &reg, "MG-010", P1, Zone::Hand, None);
        equip_object(&mut state, &reg, P1, &w2, &p).unwrap();
        let w3 = put(&mut state, &reg, "MG-010", P1, Zone::Hand, None);
        assert_eq!(
            equip_object(&mut state, &reg, P1, &w3, &p),
            Err(EngineError::illegal(
                "Char MG-005 already has max weapon equipped"
            ))
        );
    }

    #[test]
    fn equip_clima_tact_is_free_with_nami_and_usopp() {
        let reg = registry_with(vec![
            character("MG-003", 2, 1, 1, 3),
            character("MG-004", 2, 1, 1, 3),
            object("MG-012", 3, ObjectSubtype::Weapon),
        ]);
        let mut state = blank_state();
        state.players.get_mut(P1).volonte = 1;

        let nami = put(&mut state, &reg, "MG-003", P1, Zone::Board, Some(Slot::V1));
        let clima = put(&mut state, &reg, "MG-012", P1, Zone::Hand, None);
        // Only Nami on board: the full cost applies and 1 Volonte is not enough.
        assert!(matches!(
            equip_object(&mut state, &reg, P1, &clima, &nami),
            Err(EngineError::CannotAfford { cost: 3, .. })
        ));

        put(&mut state, &reg, "MG-004", P1, Zone::Board, Some(Slot::V2));
        equip_object(&mut state, &reg, P1, &clima, &nami).unwrap();
        assert_eq!(state.players.get(P1).volonte, 1);
    }

    // --- deployShip ---

    #[test]
    fn deploy_ship_replaces_the_previous_one_and_runs_its_destroy_effect() {
        let mut old = ship("MG-020", 2, "Vos Mugiwara ont +1 PV au déploiement.");
        old.ship_destroy_effect = Some(ShipDestroyEffect {
            heal_all: Some(2),
            draw: Some(1),
            deploy_token: Some("TOK".into()),
            ..Default::default()
        });
        let mut token = character("TOK", 0, 1, 0, 2);
        token.is_token = Some(true);
        let reg = registry_with(vec![
            old,
            ship("MG-021", 2, "Vos Mugiwara ont +1 ATK."),
            character("HURT", 1, 1, 1, 5),
            token,
        ]);
        let mut state = blank_state();
        let mut ctx = EngineContext::seeded(7);

        let old_id = put(&mut state, &reg, "MG-020", P1, Zone::Board, None);
        state.players.get_mut(P1).active_ship = Some(old_id.clone());
        let hurt = put(&mut state, &reg, "HURT", P1, Zone::Board, Some(Slot::V1));
        state.card_mut(&hurt).unwrap().current_pv = 1;
        let deck_card = put(&mut state, &reg, "HURT", P1, Zone::Deck, None);
        let new_id = put(&mut state, &reg, "MG-021", P1, Zone::Hand, None);

        deploy_ship(&mut state, &reg, &mut ctx, P1, &new_id).unwrap();

        assert_eq!(state.card(&hurt).unwrap().current_pv, 3); // 1 + 2, capped at 5
        assert_eq!(state.players.get(P1).hand, vec![deck_card.clone()]);
        assert_eq!(state.card(&deck_card).unwrap().zone, Zone::Hand);
        // The token took the first empty slot (V2) with the printed PV.
        let tok = state.players.get(P1).board.get(Slot::V2).cloned().unwrap();
        assert_eq!(state.card(&tok).unwrap().def_id, "TOK");
        assert_eq!(state.card(&tok).unwrap().current_pv, 2);
        assert_eq!(state.card(&tok).unwrap().deployed_turn, Some(3));
        // Old ship graveyarded, new one active.
        assert_eq!(state.card(&old_id).unwrap().zone, Zone::Graveyard);
        assert_eq!(state.players.get(P1).graveyard, vec![old_id.clone()]);
        assert_eq!(state.players.get(P1).active_ship, Some(new_id.clone()));
        assert_eq!(state.card(&new_id).unwrap().zone, Zone::Board);
        assert_eq!(
            state
                .log
                .iter()
                .map(|l| l.message.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Ship MG-020 : effet de destruction.",
                "Deploie navire Ship MG-021"
            ]
        );
    }

    #[test]
    fn ship_destroy_effect_follows_js_truthiness_for_heal_all_and_deploy_token() {
        // TS: `if (de.healAll)` / `if (de.deployToken)`. `0` and `""` are falsy,
        // so both sub-effects are skipped — the heal must not clamp a buffed
        // character back down to its printed PV, and the empty token id must
        // never reach `getCardDef("")`.
        let mut old = ship("MG-020", 2, "Vos Mugiwara ont +1 PV au deploiement.");
        old.ship_destroy_effect = Some(ShipDestroyEffect {
            heal_all: Some(0),
            draw: Some(0),
            deploy_token: Some(String::new()),
            ..Default::default()
        });
        let reg = registry_with(vec![
            old,
            ship("MG-021", 2, "Vos Mugiwara ont +1 ATK."),
            character("BUFFED", 1, 1, 1, 3),
        ]);
        let mut state = blank_state();
        let mut ctx = EngineContext::seeded(7);

        let old_id = put(&mut state, &reg, "MG-020", P1, Zone::Board, None);
        state.players.get_mut(P1).active_ship = Some(old_id.clone());
        let buffed = put(&mut state, &reg, "BUFFED", P1, Zone::Board, Some(Slot::V1));
        // A Going-Merry-style deploy bonus puts it one PV above its printed 3.
        state.card_mut(&buffed).unwrap().current_pv = 4;
        let deck_card = put(&mut state, &reg, "BUFFED", P1, Zone::Deck, None);
        let new_id = put(&mut state, &reg, "MG-021", P1, Zone::Hand, None);

        deploy_ship(&mut state, &reg, &mut ctx, P1, &new_id).unwrap();

        // healAll: 0 skipped — no `min(current, printed)` clamp.
        assert_eq!(state.card(&buffed).unwrap().current_pv, 4);
        // draw: 0 skipped.
        assert!(state.players.get(P1).hand.is_empty());
        assert_eq!(state.players.get(P1).deck, vec![deck_card]);
        // deployToken: "" skipped — no token, and no `UnknownCard("")`.
        assert_eq!(state.players.get(P1).board.get(Slot::V2), None);
        assert_eq!(state.players.get(P1).active_ship, Some(new_id));
        assert_eq!(
            state.log.first().map(|l| l.message.as_str()),
            Some("Ship MG-020 : effet de destruction.")
        );
    }

    #[test]
    fn board_errors_use_the_exact_ts_throw_messages() {
        let reg = registry_with(vec![
            character("CH", 1, 1, 1, 3),
            object("OB", 1, ObjectSubtype::Weapon),
            ship("SH", 1, "rien"),
        ]);
        let mut state = blank_state();
        let ch = put(&mut state, &reg, "CH", P1, Zone::Hand, None);
        let ob = put(&mut state, &reg, "OB", P1, Zone::Hand, None);
        let sh = put(&mut state, &reg, "SH", P1, Zone::Hand, None);
        let board_ch = put(&mut state, &reg, "CH", P1, Zone::Board, Some(Slot::V1));
        let mut ctx = EngineContext::seeded(1);

        let msg = |e: EngineError| e.to_string();

        // deployCharacter
        assert_eq!(
            msg(deploy_character(&mut state, &reg, &mut ctx, P1, "ghost", Slot::V2).unwrap_err()),
            "Card not found: ghost"
        );
        assert_eq!(
            msg(deploy_character(&mut state, &reg, &mut ctx, P2, &ch, Slot::V2).unwrap_err()),
            "Not your card"
        );
        assert_eq!(
            msg(deploy_character(&mut state, &reg, &mut ctx, P1, &board_ch, Slot::V2).unwrap_err()),
            "Card not in hand"
        );
        assert_eq!(
            msg(deploy_character(&mut state, &reg, &mut ctx, P1, &ob, Slot::V2).unwrap_err()),
            "Not a character card"
        );

        // equipObject
        assert_eq!(
            msg(equip_object(&mut state, &reg, P1, "ghost", &board_ch).unwrap_err()),
            "Object not found: ghost"
        );
        assert_eq!(
            msg(equip_object(&mut state, &reg, P2, &ob, &board_ch).unwrap_err()),
            "Not your card"
        );
        assert_eq!(
            msg(equip_object(&mut state, &reg, P1, &board_ch, &board_ch).unwrap_err()),
            "Object not in hand"
        );
        assert_eq!(
            msg(equip_object(&mut state, &reg, P1, &ch, &board_ch).unwrap_err()),
            "Not an object card"
        );
        assert_eq!(
            msg(equip_object(&mut state, &reg, P1, &ob, "ghost").unwrap_err()),
            "Target not found: ghost"
        );
        assert_eq!(
            msg(equip_object(&mut state, &reg, P1, &ob, &ch).unwrap_err()),
            "Target not on board"
        );

        // deployShip
        assert_eq!(
            msg(deploy_ship(&mut state, &reg, &mut ctx, P1, "ghost").unwrap_err()),
            "Card not found: ghost"
        );
        assert_eq!(
            msg(deploy_ship(&mut state, &reg, &mut ctx, P2, &sh).unwrap_err()),
            "Not your card"
        );
        assert_eq!(
            msg(deploy_ship(&mut state, &reg, &mut ctx, P1, &board_ch).unwrap_err()),
            "Card not in hand"
        );
        assert_eq!(
            msg(deploy_ship(&mut state, &reg, &mut ctx, P1, &ch).unwrap_err()),
            "Not a ship card"
        );

        // moveCharacter
        assert_eq!(
            msg(move_character(&mut state, P1, &ch, Slot::V2).unwrap_err()),
            "Card not on board"
        );
        assert_eq!(
            msg(move_character(&mut state, P2, &board_ch, Slot::V2).unwrap_err()),
            "Not your card"
        );
        assert_eq!(
            msg(move_character(&mut state, P1, &board_ch, Slot::A2).unwrap_err()),
            "A2 is not adjacent to V1"
        );
        state.players.get_mut(P1).used_free_move = true;
        assert_eq!(
            msg(move_character(&mut state, P1, &board_ch, Slot::V2).unwrap_err()),
            "Free move already used this turn"
        );
    }

    // --- moveCharacter ---

    #[test]
    fn move_character_only_to_an_adjacent_empty_slot_once_per_turn() {
        let reg = registry_with(vec![character("A", 1, 1, 1, 1)]);
        let mut state = blank_state();
        let a = put(&mut state, &reg, "A", P1, Zone::Board, Some(Slot::V1));
        let b = put(&mut state, &reg, "A", P1, Zone::Board, Some(Slot::V2));

        assert_eq!(
            move_character(&mut state, P1, &a, Slot::V3),
            Err(EngineError::NotAdjacent {
                from: Slot::V1,
                to: Slot::V3
            })
        );
        assert_eq!(
            move_character(&mut state, P1, &a, Slot::V2),
            Err(EngineError::SlotOccupied(Slot::V2))
        );
        move_character(&mut state, P1, &a, Slot::A1).unwrap();
        assert_eq!(state.players.get(P1).board.get(Slot::V1), None);
        assert_eq!(state.players.get(P1).board.get(Slot::A1), Some(&a));
        assert_eq!(state.card(&a).unwrap().slot, Some(Slot::A1));
        assert!(state.players.get(P1).used_free_move);
        assert!(state.log.is_empty());

        assert_eq!(
            move_character(&mut state, P1, &b, Slot::V1),
            Err(EngineError::illegal("Free move already used this turn"))
        );
    }

    // --- getValidTargets ---

    #[test]
    fn valid_targets_front_row_protection_and_range() {
        let mut ranged = character("RNG", 1, 2, 1, 3);
        ranged.traits = Some(vec![Trait::Range]);
        let reg = registry_with(vec![character("A", 1, 2, 1, 3), ranged]);
        let mut state = blank_state();

        let attacker = put(&mut state, &reg, "A", P1, Zone::Board, Some(Slot::V1));
        let back = put(&mut state, &reg, "A", P1, Zone::Board, Some(Slot::A1));
        let e_front = put(&mut state, &reg, "A", P2, Zone::Board, Some(Slot::V1));
        let e_back = put(&mut state, &reg, "A", P2, Zone::Board, Some(Slot::A2));

        // Melee from the front: only the enemy front row, captain protected.
        let t = get_valid_targets(&state, &reg, &attacker, false).unwrap();
        assert_eq!(t.character_targets, vec![e_front.clone()]);
        assert!(!t.can_target_captain);

        // Melee from the back: nothing at all.
        let t = get_valid_targets(&state, &reg, &back, false).unwrap();
        assert_eq!(t, ValidTargets::default());

        // With Range from the back: everything, in board order.
        let ranged_back = put(&mut state, &reg, "RNG", P1, Zone::Board, Some(Slot::A3));
        let t = get_valid_targets(&state, &reg, &ranged_back, false).unwrap();
        assert_eq!(t.character_targets, vec![e_front.clone(), e_back.clone()]);

        // No enemy front row → the back row (and the recto captain stays protected
        // while any character remains).
        state.players.get_mut(P2).board.set(Slot::V1, None);
        state.card_mut(&e_front).unwrap().zone = Zone::Graveyard;
        let t = get_valid_targets(&state, &reg, &attacker, false).unwrap();
        assert_eq!(t.character_targets, vec![e_back.clone()]);
        assert!(!t.can_target_captain);

        // Board wiped → the recto captain is exposed.
        state.players.get_mut(P2).board.set(Slot::A2, None);
        let t = get_valid_targets(&state, &reg, &attacker, false).unwrap();
        assert!(t.character_targets.is_empty());
        assert!(t.can_target_captain);
    }

    #[test]
    fn valid_targets_stealth_filter_and_special_attack_range() {
        let mut stealth = character("STL", 1, 2, 1, 3);
        stealth.traits = Some(vec![Trait::Stealth]);
        let mut sniper = character("SNP", 1, 2, 1, 3);
        sniper.special_attack = Some(SpecialAttack {
            name: "Tir".into(),
            cost: 1,
            atk_bonus: 1,
            attack_traits: Some(vec![AttackTrait::Range]),
            ..Default::default()
        });
        let reg = registry_with(vec![character("A", 1, 2, 1, 3), stealth, sniper]);
        let mut state = blank_state();

        let attacker = put(&mut state, &reg, "A", P1, Zone::Board, Some(Slot::V1));
        let stl = put(&mut state, &reg, "STL", P2, Zone::Board, Some(Slot::V1));
        let plain = put(&mut state, &reg, "A", P2, Zone::Board, Some(Slot::V2));

        // A non-stealth ally exists → the stealthed unit is untargetable.
        let t = get_valid_targets(&state, &reg, &attacker, false).unwrap();
        assert_eq!(t.character_targets, vec![plain.clone()]);

        // Stripping Furtif puts it back in the pool (board order).
        state
            .card_mut(&stl)
            .unwrap()
            .status_effects
            .push(StatusEffect {
                effect_type: StatusEffectType::NoStealth,
                turns_remaining: 1,
                damage_per_turn: 0,
                source: "test".into(),
            });
        let t = get_valid_targets(&state, &reg, &attacker, false).unwrap();
        assert_eq!(t.character_targets, vec![stl.clone(), plain.clone()]);

        // Only stealthed enemies left → they stay targetable.
        state.card_mut(&stl).unwrap().status_effects.clear();
        state.players.get_mut(P2).board.set(Slot::V2, None);
        state.card_mut(&plain).unwrap().zone = Zone::Graveyard;
        let t = get_valid_targets(&state, &reg, &attacker, false).unwrap();
        assert_eq!(t.character_targets, vec![stl.clone()]);

        // Range only from the special's attackTraits, and only when forSpecial.
        let sniper_back = put(&mut state, &reg, "SNP", P1, Zone::Board, Some(Slot::A1));
        assert_eq!(
            get_valid_targets(&state, &reg, &sniper_back, false).unwrap(),
            ValidTargets::default()
        );
        let t = get_valid_targets(&state, &reg, &sniper_back, true).unwrap();
        assert_eq!(t.character_targets, vec![stl.clone()]);
    }

    #[test]
    fn valid_targets_flipped_captain_is_targetable_like_a_character() {
        let reg = registry_with(vec![character("A", 1, 2, 1, 3)]);
        let mut state = blank_state();
        let attacker = put(&mut state, &reg, "A", P1, Zone::Board, Some(Slot::V1));
        put(&mut state, &reg, "A", P2, Zone::Board, Some(Slot::A1));

        // Verso captain in the back row while no enemy character holds the front:
        // hasFrontRow is false → targetable.
        {
            let cap = &mut state.players.get_mut(P2).captain;
            cap.flipped = true;
            cap.slot = Some(Slot::A2);
        }
        let t = get_valid_targets(&state, &reg, &attacker, false).unwrap();
        assert!(t.can_target_captain);

        // Verso captain in the front row: it *is* the front row, still targetable.
        state.players.get_mut(P2).captain.slot = Some(Slot::V2);
        let t = get_valid_targets(&state, &reg, &attacker, false).unwrap();
        assert!(t.can_target_captain);

        // A front-row character now protects a back-row verso captain.
        state.players.get_mut(P2).captain.slot = Some(Slot::A2);
        put(&mut state, &reg, "A", P2, Zone::Board, Some(Slot::V3));
        let t = get_valid_targets(&state, &reg, &attacker, false).unwrap();
        assert!(!t.can_target_captain);
    }

    #[test]
    fn remove_from_board_graveyards_objects_then_the_character() {
        let mut mugi = character("MG-006", 2, 1, 1, 3);
        mugi.tags = Some(vec!["mugiwara".into()]);
        let reg = registry_with(vec![
            character("A", 1, 2, 1, 3),
            object("MG-019", 1, ObjectSubtype::Accessory),
            mugi,
        ]);
        let mut state = blank_state();

        let c = put(&mut state, &reg, "A", P1, Zone::Board, Some(Slot::V1));
        let vivre = put(&mut state, &reg, "MG-019", P1, Zone::Board, Some(Slot::V1));
        state
            .card_mut(&c)
            .unwrap()
            .attached_objects
            .push(vivre.clone());
        let tutored = put(&mut state, &reg, "MG-006", P1, Zone::Deck, None);

        remove_from_board(&mut state, &reg, &c).unwrap();

        assert_eq!(state.players.get(P1).board.get(Slot::V1), None);
        assert_eq!(state.players.get(P1).hand, vec![tutored.clone()]);
        assert!(state.players.get(P1).deck.is_empty());
        assert_eq!(
            state.log[0].message,
            "Vivre Card : Char MG-006 rejoint la main."
        );
        assert_eq!(
            state.players.get(P1).graveyard,
            vec![vivre.clone(), c.clone()]
        );
        assert!(state.card(&c).unwrap().attached_objects.is_empty());
        assert_eq!(state.card(&c).unwrap().slot, None);
    }
}

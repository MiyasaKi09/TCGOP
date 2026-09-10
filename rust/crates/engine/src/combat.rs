//! Combat — port of `src/engine/combat.ts`.
//!
//! The three-step attack pipeline of the TS engine:
//! 1. **declare** ([`declare_base_attack`], [`declare_special_attack`],
//!    [`declare_fruit_special_attack`], and `captain.rs`
//!    [`crate::captain::declare_captain_base_attack`]) — taps the attacker,
//!    computes `rawDamage = max(0, ATK - DEF)` and parks a
//!    [`PendingAttack`] on the state;
//! 2. **counter window** — the defender plays a counter
//!    ([`apply_counter_reduce`], [`apply_counter_cancel`],
//!    [`apply_counter_survive`]), blocks with Bouclier ([`apply_shield_block`]),
//!    dodges with Observation Haki ([`crate::haki::use_observation_haki`]) or
//!    passes;
//! 3. **resolve** ([`resolve_attack`]) — damage, elements, on-hit control,
//!    Zone/Total spread, KO chain, win check.
//!
//! Every log string is byte-identical to the TS source (note that the TS mixes
//! accented and unaccented spellings — `degats` in the declaration lines,
//! `dégâts` in the recoil/Zone lines — reproduce them exactly as written).
//!
//! `Date.now()` and `Math.random()` are never used here; the only ambient value
//! is the `Date.now()` suffix of the Chopper transform modifier id, which comes
//! from the injected [`EngineContext`].

#![allow(clippy::collapsible_if)]
// ^ The nested `if` / `if let` blocks in this module mirror the TypeScript
// source branch for branch (see the per-function `PORT:` references). Merging
// them into let-chains would break that 1:1 reading, which is the whole point
// of the port, so the lint is turned off for this file only.

use serde::{Deserialize, Serialize};

use crate::board::{
    get_adjacent_slots, get_board_characters, get_effective_atk, get_effective_def,
    has_summoning_sickness, has_trait, remove_from_board,
};
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::passives::apply_on_ko_effects;
use crate::registry::CardRegistry;
use crate::state::{GameState, PendingAttack, transactional};
use crate::types::{
    AttackTrait, CardType, ConditionalBonus, CounterEffect, Element, HakiType, Modifier,
    ModifierDuration, ModifierStat, PassiveEffect, PlayerId, Slot, StatusEffect, StatusEffectType,
    Trait, Zone,
};

// ============================================================
// Captain attacker ids
// ============================================================

/// TS: captain attackers are identified by the synthetic id
/// `` `captain_${playerId}` `` (never a key of `state.cards`).
pub const CAPTAIN_ATTACKER_PREFIX: &str = "captain_";

/// Build the TS synthetic captain id `` `captain_${playerId}` ``.
pub fn captain_attacker_id(player_id: PlayerId) -> String {
    format!("{CAPTAIN_ATTACKER_PREFIX}{}", player_id.as_str())
}

/// TS `getAttackerOwner(state, attackerId)` — `src/engine/combat.ts:871`.
///
/// `"captain_player1"` / `"captain_player2"` decode to their player; anything
/// else is looked up in `state.cards`, and only *that* branch throws
/// `Attacker not found: {id}`.
///
/// The captain branch is `attackerId.replace("captain_", "") as PlayerId` — an
/// unchecked cast, so a synthetic id with any other suffix yields a bogus
/// `PlayerId` string and resolution simply continues. Every consumer of the
/// result either compares it against `"player1"` or funnels it through
/// `getOpponent`, which is `playerId === "player1" ? "player2" : "player1"`
/// (gameState.ts:473) — so a bogus suffix behaves exactly like the
/// non-`player1` player. [`PlayerId::Player2`] is that value, and it is what
/// this branch returns rather than raising an error TS never raises.
pub fn get_attacker_owner(state: &GameState, attacker_id: &str) -> Result<PlayerId, EngineError> {
    if let Some(rest) = attacker_id.strip_prefix(CAPTAIN_ATTACKER_PREFIX) {
        return Ok(match rest {
            "player1" => PlayerId::Player1,
            _ => PlayerId::Player2,
        });
    }
    match state.cards.get(attacker_id) {
        Some(card) => Ok(card.owner),
        None => Err(EngineError::illegal(format!(
            "Attacker not found: {attacker_id}"
        ))),
    }
}

// ============================================================
// The `survivePlayed` flag
// ============================================================

/// TS `applyCounterSurvive` writes an **extra** property on
/// `state.pendingAttack` — `(draft.pendingAttack as PendingAttack &
/// { survivePlayed?: boolean }).survivePlayed = true` — which
/// `applyCaptainDamage` / `applyCharacterDamage` read back to clamp the
/// protected target to 1 PV; see `src/engine/combat.ts:585,907,948`.
///
/// It is carried by [`PendingAttack::survive_played`], which serialises as
/// `"survivePlayed"` just like the TS property.
pub fn survive_played(state: &GameState) -> bool {
    state
        .pending_attack
        .as_ref()
        .and_then(|p| p.survive_played)
        .unwrap_or(false)
}

/// Setter half of [`survive_played`].
pub fn set_survive_played(state: &mut GameState, value: bool) {
    if let Some(pending) = state.pending_attack.as_mut() {
        pending.survive_played = if value { Some(true) } else { None };
    }
}

// ============================================================
// Step 1 — declare
// ============================================================

/// TS `attackerNoDodge(state, attackerInstanceId)` — `src/engine/combat.ts:29`.
///
/// `true` when the attacker's passive carries `noDodge` **or** it has the
/// Kabuto (`MG-013`) equipped; the declaration then sets
/// `pendingAttack.cannotBeDodged`.
pub fn attacker_no_dodge(
    state: &GameState,
    registry: &CardRegistry,
    attacker_instance_id: &str,
) -> Result<bool, EngineError> {
    let Some(card) = state.cards.get(attacker_instance_id) else {
        return Ok(false);
    };
    let def = registry.get_card_def(&card.def_id)?;
    if def.passive.as_ref().is_some_and(|p| {
        p.effects
            .iter()
            .any(|e| matches!(e, PassiveEffect::NoDodge))
    }) {
        return Ok(true);
    }
    for obj_id in &card.attached_objects {
        if let Some(obj) = state.cards.get(obj_id) {
            if registry.get_card_def(&obj.def_id)?.id == "MG-013" {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// TS `attackerStripsStealth(state, attackerInstanceId)`
/// — `src/engine/combat.ts:42`.
///
/// `true` when the attacker's passive carries `stripStealthOnAttack` (Smoker);
/// the target then gains the `noStealth` status on resolution.
pub fn attacker_strips_stealth(
    state: &GameState,
    registry: &CardRegistry,
    attacker_instance_id: &str,
) -> Result<bool, EngineError> {
    let Some(card) = state.cards.get(attacker_instance_id) else {
        return Ok(false);
    };
    let def = registry.get_card_def(&card.def_id)?;
    Ok(def.passive.as_ref().is_some_and(|p| {
        p.effects
            .iter()
            .any(|e| matches!(e, PassiveEffect::StripStealthOnAttack))
    }))
}

/// TS `conditionalAtkBonus(state, cond, targetInstanceId, targetIsCaptain, defenderId)`
/// — `src/engine/combat.ts:49`.
///
/// Against a captain only `vsFaction` can match (the captain def's faction);
/// against a character `vsFaction` is checked first, then `vsTrait` through
/// [`crate::board::has_trait`]. Returns `cond.amount` or `0`.
pub fn conditional_atk_bonus(
    state: &GameState,
    registry: &CardRegistry,
    cond: Option<&ConditionalBonus>,
    target_instance_id: &str,
    target_is_captain: bool,
    defender_id: PlayerId,
) -> Result<i32, EngineError> {
    let Some(cond) = cond else {
        return Ok(0);
    };
    if target_is_captain {
        let cap_def = registry.get_captain_def(&state.players.get(defender_id).captain.def_id)?;
        if cond.vs_faction.is_some_and(|f| cap_def.faction == f) {
            return Ok(cond.amount);
        }
        return Ok(0);
    }
    let Some(tc) = state.cards.get(target_instance_id) else {
        return Ok(0);
    };
    let tdef = registry.get_card_def(&tc.def_id)?;
    if cond.vs_faction.is_some_and(|f| tdef.faction == f) {
        return Ok(cond.amount);
    }
    if let Some(t) = cond.vs_trait {
        if has_trait(state, registry, target_instance_id, t)? {
            return Ok(cond.amount);
        }
    }
    Ok(0)
}

/// The DEF the pending attack is computed against: the opposing captain's
/// side DEF plus its `def` modifiers, or the target character's effective DEF
/// (TS repeats this block verbatim in the three `declare*` functions).
fn target_def_value(
    state: &GameState,
    registry: &CardRegistry,
    attacker_owner: PlayerId,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<i32, EngineError> {
    if !target_is_captain {
        return get_effective_def(state, registry, target_instance_id);
    }
    let opponent = attacker_owner.opponent();
    let cap = &state.players.get(opponent).captain;
    let cap_def = registry.get_captain_def(&cap.def_id)?;
    let mut target_def = if cap.flipped {
        cap_def.verso.def
    } else {
        cap_def.recto.def
    };
    for m in &cap.modifiers {
        if m.stat == ModifierStat::Def {
            target_def += m.amount;
        }
    }
    Ok(target_def)
}

/// TS `targetIsCaptain ? "Capitaine" : getCardDef(state.cards[id].defId).name`.
fn target_display_name(
    state: &GameState,
    registry: &CardRegistry,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<String, EngineError> {
    if target_is_captain {
        return Ok("Capitaine".to_string());
    }
    let card = state.get_card(target_instance_id)?;
    Ok(registry.get_card_def(&card.def_id)?.name.clone())
}

/// The `trap` pre-step shared by `declareBaseAttack` / `declareSpecialAttack`.
/// Returns `true` when the attacker died to the trap (the caller returns
/// immediately, leaving no pending attack).
fn trigger_attacker_trap(
    state: &mut GameState,
    registry: &CardRegistry,
    attacker_instance_id: &str,
    owner: PlayerId,
    def_name: &str,
) -> Result<bool, EngineError> {
    let trap = state
        .get_card(attacker_instance_id)?
        .status_effects
        .iter()
        .find(|e| e.effect_type == StatusEffectType::Trap)
        .cloned();
    let Some(trap) = trap else {
        return Ok(false);
    };
    {
        let a = state.get_card_mut(attacker_instance_id)?;
        a.current_pv -= trap.damage_per_turn;
        a.status_effects
            .retain(|e| e.effect_type != StatusEffectType::Trap);
    }
    state.add_log(
        owner,
        format!(
            "Piege ! {def_name} subit {} degats en attaquant !",
            trap.damage_per_turn
        ),
    );
    if state.get_card(attacker_instance_id)?.current_pv <= 0 {
        state.add_log(owner, format!("{def_name} est KO par le piege !"));
        remove_from_board(state, registry, attacker_instance_id)?;
        return Ok(true);
    }
    Ok(false)
}

/// TS `declareBaseAttack(state, attackerInstanceId, targetInstanceId, targetIsCaptain)`
/// — `src/engine/combat.ts:73`.
///
/// Free attack: triggers the attacker's `trap` status first (log
/// `"Piege ! {name} subit {n} degats en attaquant !"`, and on death
/// `"{name} est KO par le piege !"` + removal, returning early with no pending
/// attack), then taps the attacker, sets **both** `usedBaseAction` and
/// `usedSpecialAttack` (one action per turn), stores the [`PendingAttack`] and
/// logs `"{name} attaque {target} (ATK {atk} vs DEF {def} = {raw} degats)"`.
///
/// Errors: `Attacker not found`, `Attacker is tapped`, `Base action already used`,
/// `Character has summoning sickness`, `Character has 0 ATK — cannot attack`.
///
/// Like the TS original this is **all-or-nothing**: the TS function threads a
/// new immutable state through every step and only returns it once the last
/// step succeeded, so a `throw` leaves the caller's state untouched. The body
/// therefore runs inside [`transactional`], which restores the pre-call
/// `GameState` when it fails.
pub fn declare_base_attack(
    state: &mut GameState,
    registry: &CardRegistry,
    attacker_instance_id: &str,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<(), EngineError> {
    transactional(state, |state| {
        declare_base_attack_inner(
            state,
            registry,
            attacker_instance_id,
            target_instance_id,
            target_is_captain,
        )
    })
}

/// Body of [`declare_base_attack`], run inside [`transactional`].
fn declare_base_attack_inner(
    state: &mut GameState,
    registry: &CardRegistry,
    attacker_instance_id: &str,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<(), EngineError> {
    let Some(attacker) = state.cards.get(attacker_instance_id) else {
        return Err(EngineError::illegal("Attacker not found"));
    };
    if attacker.tapped {
        return Err(EngineError::illegal("Attacker is tapped"));
    }
    if attacker.used_base_action {
        return Err(EngineError::illegal("Base action already used"));
    }
    if has_summoning_sickness(state, registry, attacker_instance_id)? {
        return Err(EngineError::SummoningSickness);
    }

    let attacker = state.get_card(attacker_instance_id)?;
    let owner = attacker.owner;
    let attached: Vec<String> = attacker.attached_objects.clone();
    let def = registry.get_card_def(&attacker.def_id)?;
    let def_name = def.name.clone();
    let def_piercing = def.has_trait(Trait::Piercing);
    let def_has_natural_haki = def.natural_haki.as_ref().is_some_and(|h| !h.is_empty());
    let base_action = def.base_action.clone();

    if get_effective_atk(state, registry, attacker_instance_id)? <= 0 {
        return Err(EngineError::illegal("Character has 0 ATK — cannot attack"));
    }

    // Trigger trap on attacker if present
    if trigger_attacker_trap(state, registry, attacker_instance_id, owner, &def_name)? {
        return Ok(());
    }

    let atk = get_effective_atk(state, registry, attacker_instance_id)?;
    let attack_traits: Vec<AttackTrait> = base_action
        .as_ref()
        .and_then(|b| b.attack_traits.clone())
        .unwrap_or_default();

    // Check equipped objects for granted element (e.g. Baril d'Eau grants "water")
    let mut attack_element = base_action.as_ref().and_then(|b| b.element);
    for obj_id in &attached {
        if let Some(obj_card) = state.cards.get(obj_id) {
            let obj_def = registry.get_card_def(&obj_card.def_id)?;
            if let Some(el) = obj_def.grants_element {
                attack_element = Some(el);
            }
        }
    }

    // Calculate raw damage against target
    let mut target_def = target_def_value(
        state,
        registry,
        owner,
        target_instance_id,
        target_is_captain,
    )?;

    // Apply Piercing (DEF / 2)
    let is_piercing = attack_traits.contains(&AttackTrait::Piercing) || def_piercing;
    if is_piercing {
        target_def = target_def.div_euclid(2);
    }

    let raw_damage = (atk - target_def).max(0);

    // Haki to pierce Logia: natural Haki, Armament passive (T7+), or Water element.
    let has_haki = def_has_natural_haki
        || state.turn_number >= 7
        || attack_element == Some(Element::Water)
        || state.players.get(owner).has_haki_this_turn();

    let strips_stealth = attacker_strips_stealth(state, registry, attacker_instance_id)?;
    let pending = PendingAttack {
        attacker_id: attacker_instance_id.to_string(),
        target_id: target_instance_id.to_string(),
        target_is_captain,
        is_special: false,
        raw_damage,
        attack_power: Some(atk),
        element: attack_element,
        attack_traits,
        has_haki,
        ignore_shield: None,
        cannot_be_dodged: Some(attacker_no_dodge(state, registry, attacker_instance_id)?),
        immobilize: base_action.as_ref().and_then(|b| b.immobilize),
        sleep: None,
        pushback: None,
        pushback_slots: None,
        strip_stealth: Some(
            base_action
                .as_ref()
                .and_then(|b| b.strip_stealth)
                .unwrap_or(false)
                || strips_stealth,
        ),
        survive_played: None,
    };

    {
        let a = state.get_card_mut(attacker_instance_id)?;
        a.tapped = true;
        // One action per turn (Rulebook v3.1 §2.2/§6): base OR special, never both.
        a.used_base_action = true;
        a.used_special_attack = true;
    }
    let target_name = target_display_name(state, registry, target_instance_id, target_is_captain)?;
    state.pending_attack = Some(pending);

    state.add_log(
        owner,
        format!(
            "{def_name} attaque {target_name} (ATK {atk} vs DEF {target_def} = {raw_damage} degats)"
        ),
    );

    Ok(())
}

/// TS `declareSpecialAttack(state, attackerInstanceId, targetInstanceId, targetIsCaptain)`
/// — `src/engine/combat.ts:199`.
///
/// Same trap pre-step as the base attack, then pays `spec.cost`. A
/// `spec.transform` special (Chopper Monster Point) short-circuits: no pending
/// attack, an `atk` modifier `` `transform_{name}_{Date.now()}` `` sized
/// `max(0, t.atk - currentAtk)`, a `selfKO` status of `t.turns`, and the log
/// `"{name} : {spec} ! ATK {t.atk} pendant {t.turns} tours, puis KO."`.
/// Otherwise damage is `baseAtk + spec.atkBonus + conditionalAtkBonus` against
/// DEF halved by Piercing and lowered by `ignoreDef`; `twoTargets` adds the
/// `zone` attack trait.
///
/// Errors: `Attacker not found`, `Special already used`,
/// `Character has summoning sickness`, `Character has no special attack`,
/// `Already used this ability (1x/game)`, `Cannot afford special (cost {n})`.
///
/// Like the TS original this is **all-or-nothing**: the TS function threads a
/// new immutable state through every step and only returns it once the last
/// step succeeded, so a `throw` leaves the caller's state untouched. The body
/// therefore runs inside [`transactional`], which restores the pre-call
/// `GameState` when it fails.
pub fn declare_special_attack(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    attacker_instance_id: &str,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<(), EngineError> {
    transactional(state, |state| {
        declare_special_attack_inner(
            state,
            registry,
            ctx,
            attacker_instance_id,
            target_instance_id,
            target_is_captain,
        )
    })
}

/// Body of [`declare_special_attack`], run inside [`transactional`].
fn declare_special_attack_inner(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    attacker_instance_id: &str,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<(), EngineError> {
    let Some(attacker) = state.cards.get(attacker_instance_id) else {
        return Err(EngineError::illegal("Attacker not found"));
    };
    if attacker.used_special_attack {
        return Err(EngineError::illegal("Special already used"));
    }
    if has_summoning_sickness(state, registry, attacker_instance_id)? {
        return Err(EngineError::SummoningSickness);
    }

    let attacker = state.get_card(attacker_instance_id)?;
    let owner = attacker.owner;
    let def = registry.get_card_def(&attacker.def_id)?;
    let def_name = def.name.clone();
    let def_piercing = def.has_trait(Trait::Piercing);
    let def_has_natural_haki = def.natural_haki.as_ref().is_some_and(|h| !h.is_empty());
    let spec = def.special_attack.clone();

    // Trigger trap on attacker if present
    if trigger_attacker_trap(state, registry, attacker_instance_id, owner, &def_name)? {
        return Ok(());
    }

    let Some(spec) = spec else {
        return Err(EngineError::illegal("Character has no special attack"));
    };

    // Check 1x/game
    if spec.once_per_game.unwrap_or(false)
        && state
            .get_card(attacker_instance_id)?
            .used_once_abilities
            .iter()
            .any(|a| a == &spec.name)
    {
        return Err(EngineError::illegal("Already used this ability (1x/game)"));
    }

    if !state.can_afford(owner, spec.cost) {
        return Err(EngineError::illegal(format!(
            "Cannot afford special (cost {})",
            spec.cost
        )));
    }

    // Self-transformation special (Chopper Monster Point): no target, no pending attack.
    if let Some(t) = spec.transform {
        state.spend_volonte(owner, spec.cost)?;
        let cur_atk = get_effective_atk(state, registry, attacker_instance_id)?;
        {
            let c = state.get_card_mut(attacker_instance_id)?;
            c.used_base_action = true;
            c.used_special_attack = true;
            c.tapped = true;
            if spec.once_per_game.unwrap_or(false) {
                c.used_once_abilities.push(spec.name.clone());
            }
            c.modifiers.push(Modifier {
                id: format!("transform_{}_{}", spec.name, ctx.now()),
                stat: ModifierStat::Atk,
                amount: (t.atk - cur_atk).max(0),
                source: "transform".to_string(),
                duration: ModifierDuration::Permanent,
                turns_remaining: None,
            });
            // Self-KO countdown — KO'd after `turns` of the owner's turns.
            c.status_effects.push(StatusEffect {
                effect_type: StatusEffectType::SelfKo,
                turns_remaining: t.turns,
                damage_per_turn: 0,
                source: spec.name.clone(),
            });
        }
        state.add_log(
            owner,
            format!(
                "{def_name} : {} ! ATK {} pendant {} tours, puis KO.",
                spec.name, t.atk, t.turns
            ),
        );
        return Ok(());
    }

    let base_atk = get_effective_atk(state, registry, attacker_instance_id)?;
    let cond_bonus = conditional_atk_bonus(
        state,
        registry,
        spec.conditional_bonus.as_ref(),
        target_instance_id,
        target_is_captain,
        owner.opponent(),
    )?;

    state.spend_volonte(owner, spec.cost)?;

    let total_atk = base_atk + spec.atk_bonus + cond_bonus;

    // "Touche 2 cibles" is approximated as a small Zone (target + adjacents).
    let mut attack_traits: Vec<AttackTrait> = spec.attack_traits.clone().unwrap_or_default();
    if spec.two_targets.unwrap_or(false) && !attack_traits.contains(&AttackTrait::Zone) {
        attack_traits.push(AttackTrait::Zone);
    }

    let mut target_def_val = target_def_value(
        state,
        registry,
        owner,
        target_instance_id,
        target_is_captain,
    )?;

    // Piercing
    let is_piercing = attack_traits.contains(&AttackTrait::Piercing) || def_piercing;
    if is_piercing {
        target_def_val = target_def_val.div_euclid(2);
    }

    // Ignore DEF (TS truthiness: `ignoreDef: 0` is falsy — no clamp at all)
    if let Some(ignore) = spec.ignore_def.filter(|v| *v != 0) {
        target_def_val = (target_def_val - ignore).max(0);
    }

    let raw_damage = (total_atk - target_def_val).max(0);

    // Haki to pierce Logia: natural Haki, Armament passive (T7+), or Water element.
    let has_haki = def_has_natural_haki
        || state.turn_number >= 7
        || spec.element == Some(Element::Water)
        || state.players.get(owner).has_haki_this_turn();

    let strips_stealth = attacker_strips_stealth(state, registry, attacker_instance_id)?;
    let pending = PendingAttack {
        attacker_id: attacker_instance_id.to_string(),
        target_id: target_instance_id.to_string(),
        target_is_captain,
        is_special: true,
        raw_damage,
        attack_power: Some(total_atk),
        element: spec.element,
        attack_traits,
        has_haki,
        ignore_shield: spec.ignore_shield,
        cannot_be_dodged: Some(
            spec.cannot_be_dodged.unwrap_or(false)
                || attacker_no_dodge(state, registry, attacker_instance_id)?,
        ),
        immobilize: spec.immobilize,
        sleep: spec.sleep,
        pushback: Some(spec.pushback.unwrap_or(false) || spec.pushback_slots.unwrap_or(0) > 0),
        pushback_slots: None,
        strip_stealth: Some(spec.strip_stealth.unwrap_or(false) || strips_stealth),
        survive_played: None,
    };

    {
        let a = state.get_card_mut(attacker_instance_id)?;
        a.tapped = true;
        // One action per turn (Rulebook v3.1 §2.2/§6): base OR special, never both.
        a.used_special_attack = true;
        a.used_base_action = true;
        if spec.once_per_game.unwrap_or(false) {
            a.used_once_abilities.push(spec.name.clone());
        }
    }
    let target_name = target_display_name(state, registry, target_instance_id, target_is_captain)?;
    state.pending_attack = Some(pending);

    state.add_log(
        owner,
        format!(
            "{def_name} utilise {} sur {target_name} (ATK {total_atk} vs DEF {target_def_val} = {raw_damage} degats)",
            spec.name
        ),
    );

    Ok(())
}

/// TS `declareFruitSpecialAttack(state, attackerInstanceId, fruitInstanceId,
/// targetInstanceId, targetIsCaptain)` — `src/engine/combat.ts:356`.
///
/// The awakened-fruit special. No trap pre-step and no `conditionalBonus`; the
/// `hasHaki` test omits `player.hakiThisTurn` (quirk — reproduce it), and
/// `cannotBeDodged` is *not* set from [`attacker_no_dodge`].
///
/// Errors: `Attacker not found`, `Character has summoning sickness`,
/// `Fruit not awakened`, `Fruit has no awakening special attack`,
/// `Already used this fruit ability (1x/game)`,
/// `Cannot afford fruit special (cost {n})`.
///
/// Like the TS original this is **all-or-nothing**: the TS function threads a
/// new immutable state through every step and only returns it once the last
/// step succeeded, so a `throw` leaves the caller's state untouched. The body
/// therefore runs inside [`transactional`], which restores the pre-call
/// `GameState` when it fails.
pub fn declare_fruit_special_attack(
    state: &mut GameState,
    registry: &CardRegistry,
    attacker_instance_id: &str,
    fruit_instance_id: &str,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<(), EngineError> {
    transactional(state, |state| {
        declare_fruit_special_attack_inner(
            state,
            registry,
            attacker_instance_id,
            fruit_instance_id,
            target_instance_id,
            target_is_captain,
        )
    })
}

/// Body of [`declare_fruit_special_attack`], run inside [`transactional`].
fn declare_fruit_special_attack_inner(
    state: &mut GameState,
    registry: &CardRegistry,
    attacker_instance_id: &str,
    fruit_instance_id: &str,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<(), EngineError> {
    let Some(attacker) = state.cards.get(attacker_instance_id) else {
        return Err(EngineError::illegal("Attacker not found"));
    };
    let owner = attacker.owner;
    let attacker_def_id = attacker.def_id.clone();
    if has_summoning_sickness(state, registry, attacker_instance_id)? {
        return Err(EngineError::SummoningSickness);
    }

    let fruit_awakened = state
        .cards
        .get(fruit_instance_id)
        .is_some_and(|c| c.is_awakened.unwrap_or(false));
    if !fruit_awakened {
        return Err(EngineError::illegal("Fruit not awakened"));
    }

    let fruit_def = registry.get_card_def(&state.get_card(fruit_instance_id)?.def_id)?;
    let spec = fruit_def
        .fruit_effects
        .as_ref()
        .and_then(|f| f.awakening.as_ref())
        .and_then(|a| a.special_attack.clone());
    let Some(spec) = spec else {
        return Err(EngineError::illegal(
            "Fruit has no awakening special attack",
        ));
    };

    // Check once per game
    if spec.once_per_game.unwrap_or(false)
        && state
            .get_card(attacker_instance_id)?
            .used_once_abilities
            .iter()
            .any(|a| a == &spec.name)
    {
        return Err(EngineError::illegal(
            "Already used this fruit ability (1x/game)",
        ));
    }

    if !state.can_afford(owner, spec.cost) {
        return Err(EngineError::illegal(format!(
            "Cannot afford fruit special (cost {})",
            spec.cost
        )));
    }

    state.spend_volonte(owner, spec.cost)?;

    let def = registry.get_card_def(&attacker_def_id)?;
    let def_name = def.name.clone();
    let def_has_natural_haki = def.natural_haki.as_ref().is_some_and(|h| !h.is_empty());

    let base_atk = get_effective_atk(state, registry, attacker_instance_id)?;
    let total_atk = base_atk + spec.atk_bonus;

    let attack_traits: Vec<AttackTrait> = spec.attack_traits.clone().unwrap_or_default();

    let mut target_def_val = target_def_value(
        state,
        registry,
        owner,
        target_instance_id,
        target_is_captain,
    )?;
    if attack_traits.contains(&AttackTrait::Piercing) {
        target_def_val = target_def_val.div_euclid(2);
    }
    if let Some(ignore) = spec.ignore_def.filter(|v| *v != 0) {
        target_def_val = (target_def_val - ignore).max(0);
    }

    let raw_damage = (total_atk - target_def_val).max(0);

    let has_haki =
        def_has_natural_haki || state.turn_number >= 7 || spec.element == Some(Element::Water);

    let pending = PendingAttack {
        attacker_id: attacker_instance_id.to_string(),
        target_id: target_instance_id.to_string(),
        target_is_captain,
        is_special: true,
        raw_damage,
        attack_power: Some(total_atk),
        element: spec.element,
        attack_traits,
        has_haki,
        ignore_shield: spec.ignore_shield,
        cannot_be_dodged: None,
        immobilize: spec.immobilize,
        sleep: spec.sleep,
        pushback: spec.pushback,
        pushback_slots: None,
        strip_stealth: spec.strip_stealth,
        survive_played: None,
    };

    {
        let a = state.get_card_mut(attacker_instance_id)?;
        a.tapped = true;
        // One action per turn (Rulebook v3.1 §2.2/§6): base OR special, never both.
        a.used_special_attack = true;
        a.used_base_action = true;
        if spec.once_per_game.unwrap_or(false) {
            a.used_once_abilities.push(spec.name.clone());
        }
    }
    let target_name = target_display_name(state, registry, target_instance_id, target_is_captain)?;
    state.pending_attack = Some(pending);

    state.add_log(
        owner,
        format!(
            "{def_name} utilise {} sur {target_name} (ATK {total_atk} vs DEF {target_def_val} = {raw_damage} degats)",
            spec.name
        ),
    );

    Ok(())
}

// ============================================================
// Step 2 — counter window
// ============================================================

/// Move a played counter from its owner's hand to the graveyard.
fn discard_counter(state: &mut GameState, owner: PlayerId, counter_instance_id: &str) {
    let player = state.players.get_mut(owner);
    player.hand.retain(|id| id != counter_instance_id);
    if let Some(card) = state.cards.get_mut(counter_instance_id) {
        card.zone = Zone::Graveyard;
    }
    state
        .players
        .get_mut(owner)
        .graveyard
        .push(counter_instance_id.to_string());
}

/// TS `applyCounterReduce(state, counterInstanceId)` — `src/engine/combat.ts:460`.
///
/// Pays the counter's cost, discards it and lowers `pendingAttack.rawDamage`
/// by `amount` (or by `captainBonus` when the captain is the target); logs
/// `"Joue {name} : reduit les degats de {n}"`.
///
/// Errors: `No pending attack`, `Counter not found`, `Not a counter card`,
/// `Not a damage reduction counter`, `Cannot afford counter`.
pub fn apply_counter_reduce(
    state: &mut GameState,
    registry: &CardRegistry,
    counter_instance_id: &str,
) -> Result<(), EngineError> {
    if state.pending_attack.is_none() {
        return Err(EngineError::NoPendingAttack);
    }

    let Some(counter) = state.cards.get(counter_instance_id) else {
        return Err(EngineError::illegal("Counter not found"));
    };
    let owner = counter.owner;
    let counter_def = registry.get_card_def(&counter.def_id)?;
    let counter_name = counter_def.name.clone();
    let counter_cost = counter_def.cost;
    let Some(effect) = counter_def.counter_effect.clone() else {
        return Err(EngineError::illegal("Not a counter card"));
    };
    let CounterEffect::ReduceDamage {
        amount,
        captain_bonus,
    } = effect
    else {
        return Err(EngineError::illegal("Not a damage reduction counter"));
    };

    if !state.can_afford(owner, counter_cost) {
        return Err(EngineError::illegal("Cannot afford counter"));
    }

    let mut reduction = amount;
    // Captain bonus: if targeting captain, extra reduction
    let targets_captain = state
        .pending_attack
        .as_ref()
        .is_some_and(|p| p.target_is_captain);
    if targets_captain {
        // TS truthiness: a `captainBonus` of 0 is falsy and does NOT override.
        if let Some(bonus) = captain_bonus.filter(|b| *b != 0) {
            reduction = bonus;
        }
    }

    state.spend_volonte(owner, counter_cost)?;

    discard_counter(state, owner, counter_instance_id);
    if let Some(pending) = state.pending_attack.as_mut() {
        pending.raw_damage = (pending.raw_damage - reduction).max(0);
    }

    state.add_log(
        owner,
        format!("Joue {counter_name} : reduit les degats de {reduction}"),
    );
    Ok(())
}

/// TS `applyCounterCancel(state, counterInstanceId)` — `src/engine/combat.ts:514`.
///
/// Handles both `cancel` and `untargetable` counter effects: clears
/// `pendingAttack`, logs `"{name} : attaque annulée !"`, and for
/// `selfCaptainDamage` also drains the own captain
/// (`"{name} : votre Capitaine subit {n} dégâts."`) plus a win check.
///
/// Errors: `No pending attack`, `Counter not found`, `Not a cancel counter`,
/// `Cannot afford counter`, `Attacker is too strong for this counter`.
pub fn apply_counter_cancel(
    state: &mut GameState,
    registry: &CardRegistry,
    counter_instance_id: &str,
) -> Result<(), EngineError> {
    if state.pending_attack.is_none() {
        return Err(EngineError::NoPendingAttack);
    }
    let Some(counter) = state.cards.get(counter_instance_id) else {
        return Err(EngineError::illegal("Counter not found"));
    };
    let owner = counter.owner;
    let cdef = registry.get_card_def(&counter.def_id)?;
    let cdef_name = cdef.name.clone();
    let cdef_cost = cdef.cost;
    let ce = cdef.counter_effect.clone();
    let Some(ce) = ce.filter(|e| {
        matches!(
            e,
            CounterEffect::Cancel { .. } | CounterEffect::Untargetable { .. }
        )
    }) else {
        return Err(EngineError::illegal("Not a cancel counter"));
    };
    if !state.can_afford(owner, cdef_cost) {
        return Err(EngineError::illegal("Cannot afford counter"));
    }
    let (max_attacker_atk, self_captain_damage) = match &ce {
        CounterEffect::Cancel {
            max_attacker_atk,
            self_captain_damage,
            ..
        } => (*max_attacker_atk, *self_captain_damage),
        _ => (None, None),
    };
    if let Some(max) = max_attacker_atk {
        let attack_power = state
            .pending_attack
            .as_ref()
            .and_then(|p| p.attack_power)
            .unwrap_or(0);
        if attack_power > max {
            return Err(EngineError::illegal(
                "Attacker is too strong for this counter",
            ));
        }
    }

    state.spend_volonte(owner, cdef_cost)?;
    discard_counter(state, owner, counter_instance_id);
    state.pending_attack = None;
    state.add_log(owner, format!("{cdef_name} : attaque annulée !"));

    // TS truthiness: a `selfCaptainDamage` of 0 is falsy and is skipped entirely.
    if let Some(damage) = self_captain_damage.filter(|d| *d != 0) {
        state.players.get_mut(owner).captain.current_pv -= damage;
        state.add_log(
            owner,
            format!("{cdef_name} : votre Capitaine subit {damage} dégâts."),
        );
        if let Some(w) = state.check_win_condition() {
            state.winner = Some(w);
        }
    }
    Ok(())
}

/// TS `applyCounterSurvive(state, counterInstanceId)` — `src/engine/combat.ts:552`.
///
/// Marks the pending attack with [`set_survive_played`] and tags the protected
/// character with the `"survived"` once-ability; logs
/// `"Joue {name} : survie a 1 PV !"`.
///
/// Errors: `No pending attack`, `Counter not found`, `Not a survive counter`,
/// `Cannot afford counter`, `Survive only protects allies`,
/// `This character already survived once`.
pub fn apply_counter_survive(
    state: &mut GameState,
    registry: &CardRegistry,
    counter_instance_id: &str,
) -> Result<(), EngineError> {
    if state.pending_attack.is_none() {
        return Err(EngineError::NoPendingAttack);
    }

    let Some(counter) = state.cards.get(counter_instance_id) else {
        return Err(EngineError::illegal("Counter not found"));
    };
    let owner = counter.owner;
    let counter_def = registry.get_card_def(&counter.def_id)?;
    let counter_name = counter_def.name.clone();
    let counter_cost = counter_def.cost;
    if !matches!(
        counter_def.counter_effect,
        Some(CounterEffect::Survive { .. })
    ) {
        return Err(EngineError::illegal("Not a survive counter"));
    }

    if !state.can_afford(owner, counter_cost) {
        return Err(EngineError::illegal("Cannot afford counter"));
    }

    // Survive protects an ally character — once per character (Rulebook ST01 MG-026).
    let pending = state
        .pending_attack
        .as_ref()
        .ok_or(EngineError::NoPendingAttack)?;
    let protected_id = pending.target_id.clone();
    if pending.target_is_captain {
        return Err(EngineError::illegal("Survive only protects allies"));
    }
    if state
        .cards
        .get(&protected_id)
        .is_some_and(|c| c.used_once_abilities.iter().any(|a| a == "survived"))
    {
        return Err(EngineError::illegal("This character already survived once"));
    }

    state.spend_volonte(owner, counter_cost)?;

    discard_counter(state, owner, counter_instance_id);
    // Mark the pending attack — target survives at 1 PV — and tag the character.
    set_survive_played(state, true);
    if let Some(pc) = state.cards.get_mut(&protected_id) {
        pc.used_once_abilities.push("survived".to_string());
    }

    state.add_log(owner, format!("Joue {counter_name} : survie a 1 PV !"));
    Ok(())
}

/// TS `applyShieldBlock(state, blockerInstanceId)` — `src/engine/combat.ts:603`.
///
/// The Bouclier reaction: taps the blocker, retargets the pending attack at it
/// and recomputes `rawDamage = max(0, attackPower - blockerDef - blockDamageReduction)`
/// (blocker DEF halved by Piercing). Logs `"🛡 {name} bloque l'attaque ! (Bouclier)"`.
///
/// Errors: `No pending attack`, `This attack ignores Bouclier`,
/// `Blocker not found`, `A tapped character cannot use Bouclier`,
/// `Not a Bouclier character`.
pub fn apply_shield_block(
    state: &mut GameState,
    registry: &CardRegistry,
    blocker_instance_id: &str,
) -> Result<(), EngineError> {
    let Some(pending) = state.pending_attack.clone() else {
        return Err(EngineError::NoPendingAttack);
    };
    if pending.ignore_shield.unwrap_or(false) {
        return Err(EngineError::illegal("This attack ignores Bouclier"));
    }

    let Some(blocker) = state.cards.get(blocker_instance_id) else {
        return Err(EngineError::illegal("Blocker not found"));
    };
    if blocker.tapped {
        return Err(EngineError::illegal(
            "A tapped character cannot use Bouclier",
        ));
    }
    if !has_trait(state, registry, blocker_instance_id, Trait::Shield)? {
        return Err(EngineError::illegal("Not a Bouclier character"));
    }

    let mut blocker_def = get_effective_def(state, registry, blocker_instance_id)?;
    if pending.attack_traits.contains(&AttackTrait::Piercing) {
        blocker_def = blocker_def.div_euclid(2);
    }
    // Some blockers reduce the damage further (Sentomaru, Garp).
    let blocker = state.get_card(blocker_instance_id)?;
    let blocker_owner = blocker.owner;
    let block_def = registry.get_card_def(&blocker.def_id)?;
    let blocker_name = block_def.name.clone();
    let block_red: i32 = block_def
        .passive
        .as_ref()
        .map(|p| {
            p.effects
                .iter()
                .map(|e| match e {
                    PassiveEffect::BlockDamageReduction { amount } => *amount,
                    _ => 0,
                })
                .sum()
        })
        .unwrap_or(0);
    let atk_power = pending.attack_power.unwrap_or(pending.raw_damage);
    let new_raw = (atk_power - blocker_def - block_red).max(0);

    state.get_card_mut(blocker_instance_id)?.tapped = true;
    if let Some(p) = state.pending_attack.as_mut() {
        p.target_id = blocker_instance_id.to_string();
        p.target_is_captain = false;
        p.raw_damage = new_raw;
    }

    state.add_log(
        blocker_owner,
        format!("🛡 {blocker_name} bloque l'attaque ! (Bouclier)"),
    );
    Ok(())
}

/// TS `getEligibleCounters(state, playerId)` — `src/engine/combat.ts:1134`.
///
/// The counter cards in hand (hand order) that are affordable, filtered by the
/// `cancel` counters' `maxAttackerAtk` against `pendingAttack.attackPower`.
/// Empty while there is no pending attack.
pub fn get_eligible_counters(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Result<Vec<String>, EngineError> {
    let Some(pending) = state.pending_attack.as_ref() else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for id in &state.players.get(player_id).hand {
        let card = state.get_card(id)?;
        let def = registry.get_card_def(&card.def_id)?;
        if def.card_type != CardType::Counter || !state.can_afford(player_id, def.cost) {
            continue;
        }
        if let Some(CounterEffect::Cancel {
            max_attacker_atk: Some(max),
            ..
        }) = &def.counter_effect
        {
            if pending.attack_power.unwrap_or(0) <= *max {
                out.push(id.clone());
            }
            continue;
        }
        out.push(id.clone());
    }
    Ok(out)
}

/// A single option the defender may take in the counter window — the union the
/// TS engine expresses as loose `GameAction`s in `getValidActions`
/// (`src/engine/turnManager.ts:836-874`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CounterOption {
    /// `{ type: "playCounter", instanceId }` — a counter card in hand.
    PlayCounter { instance_id: String },
    /// `{ type: "useShield", blockerInstanceId }` — an untapped adjacent Bouclier ally.
    UseShield { blocker_instance_id: String },
    /// `{ type: "useHaki", hakiType: "observation" }` — the dodge.
    Dodge { haki_type: HakiType },
    /// `{ type: "passCounter" }` — always available.
    Pass,
}

// ============================================================
// Step 3 — resolve
// ============================================================

/// TS `resolveAttack(state)` — `src/engine/combat.ts:648`.
///
/// Applies the damage ([`apply_captain_damage`] / [`apply_character_damage`]),
/// then, in this exact order: `meleeRecoil` thorns on a non-`range` attacker,
/// [`apply_element_effects`], the element-KO sweep on the primary target, the
/// thunder-propagation KO on the first occupied adjacent slot, the on-hit
/// control effects (`immobilize` 2t, `sleep` 3t, `noStealth` 2t, `pushback` to
/// the V→A slot behind), the Zone/Total spread via [`apply_spread_hit`], then
/// clears `pendingAttack` and sets `winner` from `check_win_condition`.
///
/// Errors: `No pending attack`.
pub fn resolve_attack(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
) -> Result<(), EngineError> {
    let Some(pending) = state.pending_attack.clone() else {
        return Err(EngineError::NoPendingAttack);
    };
    let survive = survive_played(state);

    // The TS reads several values off the *entry* state (`state`), not off the
    // running `next` — most importantly the primary target's slot, which the
    // KO removal would already have cleared.
    let entry_target_slot = state.cards.get(&pending.target_id).and_then(|c| c.slot);

    if pending.target_is_captain {
        apply_captain_damage(state, registry, &pending, survive)?;
    } else {
        apply_character_damage(state, registry, ctx, &pending, survive)?;
    }

    // Thorns / melee recoil (Miss Doublefinger): a melee attacker takes damage.
    if !pending.target_is_captain && !pending.attack_traits.contains(&AttackTrait::Range) {
        let tdef_pre = registry.get_card_def(&state.get_card(&pending.target_id)?.def_id)?;
        let recoil: i32 = tdef_pre
            .passive
            .as_ref()
            .map(|p| {
                p.effects
                    .iter()
                    .map(|e| match e {
                        PassiveEffect::MeleeRecoil { amount } => *amount,
                        _ => 0,
                    })
                    .sum()
            })
            .unwrap_or(0);
        let atk_id = pending.attacker_id.clone();
        let attacker_on_board = state
            .cards
            .get(&atk_id)
            .is_some_and(|c| c.zone == Zone::Board);
        if recoil > 0 && !atk_id.starts_with(CAPTAIN_ATTACKER_PREFIX) && attacker_on_board {
            let atk_card = state.get_card(&atk_id)?;
            let atk_owner = atk_card.owner;
            let atk_def_id = atk_card.def_id.clone();
            let atk_def = registry.get_card_def(&atk_def_id)?;
            let atk_name = atk_def.name.clone();
            if !atk_def.has_trait(Trait::Range) {
                state.get_card_mut(&atk_id)?.current_pv -= recoil;
                state.add_log(
                    atk_owner,
                    format!("{atk_name} subit {recoil} dégâts (Épines) !"),
                );
                if state.get_card(&atk_id)?.current_pv <= 0 {
                    let ko_def_id = state.get_card(&atk_id)?.def_id.clone();
                    let ko_owner = state.get_card(&atk_id)?.owner;
                    state.add_log(ko_owner, format!("{atk_name} est KO (Épines) !"));
                    state.grant_ko_bonus(ko_owner);
                    remove_from_board(state, registry, &atk_id)?;
                    apply_on_ko_effects(
                        state,
                        registry,
                        ctx,
                        ko_owner,
                        ko_owner.opponent(),
                        &ko_def_id,
                    )?;
                }
            }
        }
    }

    // Apply element effects
    apply_element_effects(state, registry, &pending)?;

    // Check KO from element effects (thunder propagation, sand, water x2)
    if !pending.target_is_captain {
        let ko = state.cards.get(&pending.target_id).and_then(|t| {
            if t.zone == Zone::Board && t.current_pv <= 0 {
                Some((t.owner, t.def_id.clone()))
            } else {
                None
            }
        });
        if let Some((ko_owner, ko_def_id)) = ko {
            let name = registry.get_card_def(&ko_def_id)?.name.clone();
            let atk_owner = get_attacker_owner(state, &pending.attacker_id)?;
            state.add_log(ko_owner, format!("{name} est KO (effet elementaire) !"));
            // +2 Vol. goes to the player who LOST the ally (Rulebook v3.1 §4).
            state.grant_ko_bonus(ko_owner);
            remove_from_board(state, registry, &pending.target_id)?;
            apply_on_ko_effects(state, registry, ctx, ko_owner, atk_owner, &ko_def_id)?;
        }
    }

    // Check KO from thunder propagation on adjacent characters
    if pending.element == Some(Element::Thunder) && !pending.target_is_captain {
        if let Some(slot) = entry_target_slot {
            let opponent_id = get_attacker_owner(state, &pending.attacker_id)?.opponent();
            for adj_slot in get_adjacent_slots(slot) {
                let adj_id = state.players.get(opponent_id).board.get(*adj_slot).cloned();
                if let Some(adj_id) = adj_id {
                    let ko = state.cards.get(&adj_id).and_then(|c| {
                        if c.zone == Zone::Board && c.current_pv <= 0 {
                            Some((c.owner, c.def_id.clone()))
                        } else {
                            None
                        }
                    });
                    if let Some((adj_ko_owner, adj_ko_def_id)) = ko {
                        let adj_name = registry.get_card_def(&adj_ko_def_id)?.name.clone();
                        state.add_log(adj_ko_owner, format!("{adj_name} est KO (foudre) !"));
                        // +2 Vol. goes to the player who LOST the ally (Rulebook v3.1 §4).
                        state.grant_ko_bonus(adj_ko_owner);
                        remove_from_board(state, registry, &adj_id)?;
                        let killer = get_attacker_owner(state, &pending.attacker_id)?;
                        apply_on_ko_effects(
                            state,
                            registry,
                            ctx,
                            adj_ko_owner,
                            killer,
                            &adj_ko_def_id,
                        )?;
                    }
                    break;
                }
            }
        }
    }

    // On-hit control effects on the surviving primary target.
    if !pending.target_is_captain {
        let tgt = state.cards.get(&pending.target_id).and_then(|t| {
            if t.zone == Zone::Board {
                Some((t.owner, t.def_id.clone(), t.slot))
            } else {
                None
            }
        });
        if let Some((tgt_owner, tgt_def_id, tgt_slot)) = tgt {
            let tdef = registry.get_card_def(&tgt_def_id)?;
            let tdef_name = tdef.name.clone();
            let effects = tdef
                .passive
                .as_ref()
                .map(|p| p.effects.clone())
                .unwrap_or_default();
            let ctrl_immune = effects
                .iter()
                .any(|e| matches!(e, PassiveEffect::ImmuneControl));
            let impact_immune = effects
                .iter()
                .any(|e| matches!(e, PassiveEffect::ImmuneImpact));

            if pending.immobilize.unwrap_or(false) && !ctrl_immune {
                state
                    .get_card_mut(&pending.target_id)?
                    .status_effects
                    .push(StatusEffect {
                        effect_type: StatusEffectType::Immobilize,
                        turns_remaining: 2,
                        damage_per_turn: 0,
                        source: pending.attacker_id.clone(),
                    });
                state.add_log(tgt_owner, format!("{tdef_name} est immobilisé !"));
            }
            if pending.sleep.unwrap_or(false) && !ctrl_immune {
                state
                    .get_card_mut(&pending.target_id)?
                    .status_effects
                    .push(StatusEffect {
                        effect_type: StatusEffectType::Sleep,
                        turns_remaining: 3,
                        damage_per_turn: 0,
                        source: pending.attacker_id.clone(),
                    });
                state.add_log(tgt_owner, format!("{tdef_name} est endormi !"));
            }
            if pending.strip_stealth.unwrap_or(false) {
                state
                    .get_card_mut(&pending.target_id)?
                    .status_effects
                    .push(StatusEffect {
                        effect_type: StatusEffectType::NoStealth,
                        turns_remaining: 2,
                        damage_per_turn: 0,
                        source: pending.attacker_id.clone(),
                    });
            }
            if pending.pushback.unwrap_or(false) && !impact_immune {
                let dest = match tgt_slot {
                    Some(Slot::V1) => Some(Slot::A1),
                    Some(Slot::V2) => Some(Slot::A2),
                    Some(Slot::V3) => Some(Slot::A3),
                    _ => None,
                };
                if let (Some(slot), Some(dest)) = (tgt_slot, dest) {
                    if state.players.get(tgt_owner).board.get(dest).is_none() {
                        {
                            let board = &mut state.players.get_mut(tgt_owner).board;
                            board.set(slot, None);
                            board.set(dest, Some(pending.target_id.clone()));
                        }
                        state.get_card_mut(&pending.target_id)?.slot = Some(dest);
                        state.add_log(
                            tgt_owner,
                            format!("{tdef_name} est repoussé en {} (Impact) !", dest.as_str()),
                        );
                    }
                }
            }
        }
    }

    // Zone / Total spread (Rulebook v3.1 §8 Family 2): after the primary hit, the
    // same attack also strikes the target's adjacents (Zone) or every enemy (Total).
    if !pending.target_is_captain
        && (pending.attack_traits.contains(&AttackTrait::Zone)
            || pending.attack_traits.contains(&AttackTrait::Total))
    {
        let attacker_owner = get_attacker_owner(state, &pending.attacker_id)?;
        let defender_id = attacker_owner.opponent();
        let mut secondary_ids: Vec<String> = Vec::new();

        if pending.attack_traits.contains(&AttackTrait::Total) {
            for c in get_board_characters(state, defender_id) {
                if c.instance_id != pending.target_id {
                    secondary_ids.push(c.instance_id.clone());
                }
            }
        } else if let Some(primary_slot) = entry_target_slot {
            for adj_slot in get_adjacent_slots(primary_slot) {
                if let Some(adj_id) = state.players.get(defender_id).board.get(*adj_slot) {
                    if adj_id != &pending.target_id {
                        secondary_ids.push(adj_id.clone());
                    }
                }
            }
        }

        for sid in secondary_ids {
            apply_spread_hit(state, registry, ctx, &pending, &sid)?;
        }
    }

    // Clear pending attack
    state.pending_attack = None;

    // Check win condition
    if let Some(winner) = state.check_win_condition() {
        state.winner = Some(winner);
    }

    Ok(())
}

/// TS `applySpreadHit(state, pending, targetId)` — `src/engine/combat.ts:818`.
///
/// One Zone/Total secondary hit: skipped for a Logia target without Haki,
/// otherwise `max(0, attackPower - DEF)` (DEF halved by Piercing) with the log
/// `"{name} subit {n} degats (Zone/Total) (PV: {pv})"`, the element re-applied
/// (thunder suppressed so it cannot chain), and the full KO chain
/// (`"{name} est KO !"` + `grant_ko_bonus` + `apply_on_ko_effects`).
pub fn apply_spread_hit(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    pending: &PendingAttack,
    target_id: &str,
) -> Result<(), EngineError> {
    let Some(target) = state.cards.get(target_id) else {
        return Ok(());
    };
    if target.zone != Zone::Board || target.current_pv <= 0 {
        return Ok(());
    }
    let target_def_id = target.def_id.clone();

    let attacker_owner = get_attacker_owner(state, &pending.attacker_id)?;
    let atk_power = pending.attack_power.unwrap_or(pending.raw_damage);

    // Logia intangibility blocks the spread unless the attack carries Haki/Water.
    if has_trait(state, registry, target_id, Trait::Logia)? && !pending.has_haki {
        return Ok(());
    }

    let mut target_def_val = get_effective_def(state, registry, target_id)?;
    if pending.attack_traits.contains(&AttackTrait::Piercing) {
        target_def_val = target_def_val.div_euclid(2);
    }
    let dmg = (atk_power - target_def_val).max(0);
    let target_name = registry.get_card_def(&target_def_id)?.name.clone();

    state.get_card_mut(target_id)?.current_pv -= dmg;
    let pv = state.get_card(target_id)?.current_pv;
    state.add_log(
        attacker_owner,
        format!("{target_name} subit {dmg} degats (Zone/Total) (PV: {pv})"),
    );

    // Element status on this target (skip thunder re-propagation to avoid chains).
    let mut spread_pending = pending.clone();
    spread_pending.target_id = target_id.to_string();
    spread_pending.target_is_captain = false;
    spread_pending.raw_damage = dmg;
    if pending.element == Some(Element::Thunder) {
        spread_pending.element = None;
    }
    apply_element_effects(state, registry, &spread_pending)?;

    let ko = state.cards.get(target_id).and_then(|after| {
        if after.zone == Zone::Board && after.current_pv <= 0 {
            Some((after.owner, after.def_id.clone()))
        } else {
            None
        }
    });
    if let Some((ko_owner, ko_def_id)) = ko {
        state.add_log(ko_owner, format!("{target_name} est KO !"));
        state.grant_ko_bonus(ko_owner);
        remove_from_board(state, registry, target_id)?;
        apply_on_ko_effects(state, registry, ctx, ko_owner, attacker_owner, &ko_def_id)?;
    }

    Ok(())
}

/// TS `applyCaptainDamage(state, pending)` — `src/engine/combat.ts:881`.
///
/// A flipped Logia captain with no Haki on the attack ignores the hit entirely
/// (log `"⚠ {name} : INTANGIBILITE LOGIA ! L'attaque passe a travers. Utilisez
/// le Haki (T7+) ou l'Eau pour le toucher."`). Otherwise subtracts
/// `pending.rawDamage`, clamps to 1 PV when `survivePlayed`, and logs
/// `"Capitaine {name} subit {n} degats (PV: {pv})"`.
pub fn apply_captain_damage(
    state: &mut GameState,
    registry: &CardRegistry,
    pending: &PendingAttack,
    survive_played: bool,
) -> Result<(), EngineError> {
    let attacker_owner = get_attacker_owner(state, &pending.attacker_id)?;
    let opponent_id = attacker_owner.opponent();
    let cap = &state.players.get(opponent_id).captain;
    let cap_flipped = cap.flipped;
    let cap_def = registry.get_captain_def(&cap.def_id)?;
    let cap_name = cap_def.name.clone();

    // Logia check on captain
    if cap_flipped {
        let is_logia = cap_def
            .verso
            .traits
            .as_ref()
            .is_some_and(|t| t.contains(&Trait::Logia));
        if is_logia && !pending.has_haki && pending.raw_damage > 0 {
            state.add_log(
                attacker_owner,
                format!(
                    "⚠ {cap_name} : INTANGIBILITE LOGIA ! L'attaque passe a travers. Utilisez le Haki (T7+) ou l'Eau pour le toucher."
                ),
            );
            return Ok(());
        }
    }

    let damage = pending.raw_damage;

    let target_cap = &mut state.players.get_mut(opponent_id).captain;
    target_cap.current_pv -= damage;
    if survive_played && target_cap.current_pv <= 0 {
        target_cap.current_pv = 1;
    }
    let pv = target_cap.current_pv;
    state.add_log(
        attacker_owner,
        format!("Capitaine {cap_name} subit {damage} degats (PV: {pv})"),
    );
    Ok(())
}

/// TS `applyCharacterDamage(state, pending)` — `src/engine/combat.ts:918`.
///
/// Logia intangibility (once per turn, `logiaUsedThisTurn`, log
/// `"⚠ {name} : INTANGIBILITE LOGIA ! Utilisez le Haki (T7+) ou l'Eau."`),
/// then the damage + `"{name} subit {n} degats (PV: {pv})"`. On a KO the
/// Chapeau de Paille (`RH-014`) saves the bearer once at 1 PV
/// (`"{name} survit grâce au Chapeau de Paille (1 PV) !"`); otherwise
/// `"{name} est KO !"`, `grant_ko_bonus(owner)`, `remove_from_board` and
/// `apply_on_ko_effects(koOwner, attackerOwner, koDefId)`.
pub fn apply_character_damage(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    pending: &PendingAttack,
    survive_played: bool,
) -> Result<(), EngineError> {
    let Some(target) = state.cards.get(&pending.target_id) else {
        return Ok(());
    };
    let target_owner = target.owner;
    let target_def_id = target.def_id.clone();
    let logia_used_this_turn = target.logia_used_this_turn.unwrap_or(false);
    let target_name = registry.get_card_def(&target_def_id)?.name.clone();

    // Logia check (includes traits from equipped Devil Fruits)
    let is_logia = has_trait(state, registry, &pending.target_id, Trait::Logia)?;
    if is_logia && !pending.has_haki && pending.raw_damage > 0 {
        // Check if logia already used this turn
        if !logia_used_this_turn {
            let attacker_owner = get_attacker_owner(state, &pending.attacker_id)?;
            state.get_card_mut(&pending.target_id)?.logia_used_this_turn = Some(true);
            state.add_log(
                attacker_owner,
                format!("⚠ {target_name} : INTANGIBILITE LOGIA ! Utilisez le Haki (T7+) ou l'Eau."),
            );
            return Ok(());
        }
    }

    let damage = pending.raw_damage;

    {
        let t = state.get_card_mut(&pending.target_id)?;
        t.current_pv -= damage;
        if survive_played && t.current_pv <= 0 {
            t.current_pv = 1;
        }
    }

    let attacker_owner = get_attacker_owner(state, &pending.attacker_id)?;
    let pv = state.get_card(&pending.target_id)?.current_pv;
    state.add_log(
        attacker_owner,
        format!("{target_name} subit {damage} degats (PV: {pv})"),
    );

    // Check KO
    if pv <= 0 {
        // Chapeau de Paille (RH-014): the first time the bearer would be KO'd,
        // it survives at 1 PV.
        let bearer = state.get_card(&pending.target_id)?;
        let has_strawhat = bearer
            .attached_objects
            .iter()
            .any(|id| state.cards.get(id).is_some_and(|c| c.def_id == "RH-014"));
        let used_strawhat = bearer.used_once_abilities.iter().any(|a| a == "strawhat");
        if has_strawhat && !used_strawhat {
            {
                let b = state.get_card_mut(&pending.target_id)?;
                b.current_pv = 1;
                b.used_once_abilities.push("strawhat".to_string());
            }
            state.add_log(
                target_owner,
                format!("{target_name} survit grâce au Chapeau de Paille (1 PV) !"),
            );
            return Ok(());
        }
        state.add_log(target_owner, format!("{target_name} est KO !"));
        // +2 Vol. goes to the player who LOST the ally (Rulebook v3.1 §4), not the attacker.
        state.grant_ko_bonus(target_owner);
        remove_from_board(state, registry, &pending.target_id)?;
        // Apply on-KO effects (captain bonuses, synergy rage, recalculate buffs)
        apply_on_ko_effects(
            state,
            registry,
            ctx,
            target_owner,
            attacker_owner,
            &target_def_id,
        )?;
    }

    Ok(())
}

/// TS `applyElementEffects(state, pending)` — `src/engine/combat.ts:990`.
///
/// fire → `burn` 2t/1dmg; ice → `freeze` 2t; thunder → `max(1, floor(raw/2))`
/// to the **first** occupied adjacent enemy slot only; poison → permanent
/// `poison` 1dmg; sand → −1 PV; water → the raw damage applied a second time
/// when the target is Cursed. Captain targets delegate to
/// [`apply_element_to_captain`]. No log lines.
pub fn apply_element_effects(
    state: &mut GameState,
    registry: &CardRegistry,
    pending: &PendingAttack,
) -> Result<(), EngineError> {
    let Some(element) = pending.element else {
        return Ok(());
    };
    if pending.target_is_captain {
        // Apply element to captain
        return apply_element_to_captain(state, pending);
    }

    let Some(target) = state.cards.get(&pending.target_id) else {
        return Ok(());
    };
    if target.zone != Zone::Board {
        return Ok(());
    }
    let target_slot = target.slot;

    match element {
        // Burn: 1 dmg/turn for 2 turns
        Element::Fire => {
            state
                .get_card_mut(&pending.target_id)?
                .status_effects
                .push(StatusEffect {
                    effect_type: StatusEffectType::Burn,
                    turns_remaining: 2,
                    damage_per_turn: 1,
                    source: pending.attacker_id.clone(),
                });
        }
        // Freeze: target loses next action. turnsRemaining: 2 so it survives the
        // start-of-turn decrement and blocks for 1 full turn.
        Element::Ice => {
            state
                .get_card_mut(&pending.target_id)?
                .status_effects
                .push(StatusEffect {
                    effect_type: StatusEffectType::Freeze,
                    turns_remaining: 2,
                    damage_per_turn: 0,
                    source: pending.attacker_id.clone(),
                });
        }
        // Propagate damage to 1 adjacent
        Element::Thunder => {
            if let Some(slot) = target_slot {
                let opponent = get_attacker_owner(state, &pending.attacker_id)?.opponent();
                for adj_slot in get_adjacent_slots(slot) {
                    let adj_id = state.players.get(opponent).board.get(*adj_slot).cloned();
                    if let Some(adj_id) = adj_id {
                        let propagate_dmg = pending.raw_damage.div_euclid(2).max(1);
                        if let Some(adj_card) = state.cards.get_mut(&adj_id) {
                            adj_card.current_pv -= propagate_dmg;
                        }
                        // Only propagate to 1
                        break;
                    }
                }
            }
        }
        // 1 dmg/turn permanent, can't kill
        Element::Poison => {
            state
                .get_card_mut(&pending.target_id)?
                .status_effects
                .push(StatusEffect {
                    effect_type: StatusEffectType::Poison,
                    turns_remaining: -1, // permanent
                    damage_per_turn: 1,
                    source: pending.attacker_id.clone(),
                });
        }
        // -1 PV permanent (irrecoverable)
        Element::Sand => {
            state.get_card_mut(&pending.target_id)?.current_pv -= 1;
        }
        // x2 damage vs Cursed — apply the raw damage a second time
        Element::Water => {
            if has_trait(state, registry, &pending.target_id, Trait::Cursed)? {
                if let Some(t) = state.cards.get_mut(&pending.target_id) {
                    if t.zone == Zone::Board {
                        t.current_pv -= pending.raw_damage; // double = apply again
                    }
                }
            }
        }
    }

    Ok(())
}

/// TS `applyElementToCaptain(state, pending)` — `src/engine/combat.ts:1089`.
///
/// Only `fire` (`burn` 2t/1dmg) and `ice` (`freeze` 2t) are handled; every
/// other element is a no-op on a captain.
pub fn apply_element_to_captain(
    state: &mut GameState,
    pending: &PendingAttack,
) -> Result<(), EngineError> {
    let attacker_owner = get_attacker_owner(state, &pending.attacker_id)?;
    let opponent_id = attacker_owner.opponent();

    match pending.element {
        Some(Element::Fire) => {
            state
                .players
                .get_mut(opponent_id)
                .captain
                .status_effects
                .push(StatusEffect {
                    effect_type: StatusEffectType::Burn,
                    turns_remaining: 2,
                    damage_per_turn: 1,
                    source: pending.attacker_id.clone(),
                });
        }
        Some(Element::Ice) => {
            state
                .players
                .get_mut(opponent_id)
                .captain
                .status_effects
                .push(StatusEffect {
                    effect_type: StatusEffectType::Freeze,
                    // 2 turns so it survives the start-of-turn decrement and
                    // actually skips the captain's next action.
                    turns_remaining: 2,
                    damage_per_turn: 0,
                    source: pending.attacker_id.clone(),
                });
        }
        // Other elements on captain — simplified for MVP
        _ => {}
    }

    Ok(())
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::state::{Board, CaptainInstance, CardInstance, PlayerState, Players};
    use crate::types::{
        BaseAction, CaptainDef, CaptainRecto, CaptainVerso, CardDef, CounterEffect, EntryEffect,
        Faction, FlipCondition, PassiveDef, Phase, Rarity, SpecialAttack,
    };

    fn passive(name: &str, effects: Vec<PassiveEffect>) -> PassiveDef {
        PassiveDef {
            name: name.to_string(),
            description: String::new(),
            effects,
        }
    }

    fn captain_def(id: &str, name: &str, def: i32, logia_verso: bool) -> CaptainDef {
        CaptainDef {
            id: id.to_string(),
            name: name.to_string(),
            faction: Faction::Pirate,
            tags: None,
            traits: None,
            recto: CaptainRecto {
                pv: 20,
                atk: 3,
                def,
                passive: passive("R", vec![]),
                attacks: vec![],
                surcharge: None,
            },
            flip_condition: FlipCondition::default(),
            verso: CaptainVerso {
                pv: 20,
                atk: 5,
                def,
                passive: passive("V", vec![]),
                entry_effect: EntryEffect::Draw { amount: 1 },
                base_action: BaseAction {
                    name: "Coup".to_string(),
                    atk: 5,
                    ..Default::default()
                },
                special_attack: SpecialAttack {
                    name: "Spe".to_string(),
                    cost: 3,
                    atk_bonus: 2,
                    ..Default::default()
                },
                surcharge: None,
                traits: if logia_verso {
                    Some(vec![Trait::Logia])
                } else {
                    None
                },
                natural_haki: None,
            },
        }
    }

    fn character(id: &str, name: &str, atk: i32, def: i32, pv: i32) -> CardDef {
        let mut c = CardDef::new(
            id,
            name,
            CardType::Character,
            2,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        c.atk = Some(atk);
        c.def = Some(def);
        c.pv = Some(pv);
        c.base_action = Some(BaseAction {
            name: "Frappe".to_string(),
            atk,
            ..Default::default()
        });
        c
    }

    fn counter(id: &str, name: &str, cost: i32, effect: CounterEffect) -> CardDef {
        let mut c = CardDef::new(
            id,
            name,
            CardType::Counter,
            cost,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        c.counter_effect = Some(effect);
        c
    }

    fn player(id: PlayerId, captain_id: &str) -> PlayerState {
        PlayerState {
            id,
            captain: CaptainInstance::new(captain_id.to_string(), id, 20),
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

    /// Registry + empty board state (turn 3 so no free T7 Haki).
    fn setup() -> (GameState, CardRegistry) {
        let mut reg = CardRegistry::new();
        reg.register_captain(captain_def("CAP-1", "Barbe", 1, false));
        reg.register_captain(captain_def("CAP-L", "Logia", 1, true));
        reg.register_set(vec![
            character("C-ATK", "Attaquant", 6, 0, 8),
            character("C-DEF", "Defenseur", 2, 5, 6),
            character("C-ADJ", "Voisin", 2, 0, 3),
            counter(
                "X-RED",
                "Esquive",
                1,
                CounterEffect::ReduceDamage {
                    amount: 2,
                    captain_bonus: Some(4),
                },
            ),
            counter(
                "X-SURV",
                "Survie",
                1,
                CounterEffect::Survive {
                    description: String::new(),
                },
            ),
            counter(
                "X-CANCEL",
                "Faible",
                1,
                CounterEffect::Cancel {
                    description: String::new(),
                    max_attacker_atk: Some(4),
                    self_captain_damage: None,
                    once: None,
                },
            ),
        ]);

        let state = GameState {
            cards: BTreeMap::new(),
            players: Players {
                player1: player(PlayerId::Player1, "CAP-1"),
                player2: player(PlayerId::Player2, "CAP-1"),
            },
            turn_number: 3,
            current_player: PlayerId::Player1,
            phase: Phase::Main,
            pending_attack: None,
            log: Vec::new(),
            winner: None,
            first_player: PlayerId::Player1,
        };
        (state, reg)
    }

    /// Put a fresh instance of `def_id` on `owner`'s `slot`, deployed long ago.
    fn place(
        state: &mut GameState,
        reg: &CardRegistry,
        def_id: &str,
        owner: PlayerId,
        slot: Slot,
    ) -> String {
        let pv = reg.get_card_def(def_id).unwrap().pv.unwrap_or(0);
        let instance_id = format!("{def_id}@{}:{}", owner.as_str(), slot.as_str());
        let mut inst = CardInstance::new(instance_id.clone(), def_id.to_string(), owner, pv);
        inst.zone = Zone::Board;
        inst.slot = Some(slot);
        inst.deployed_turn = Some(0);
        state.cards.insert(instance_id.clone(), inst);
        state
            .players
            .get_mut(owner)
            .board
            .set(slot, Some(instance_id.clone()));
        instance_id
    }

    /// Put a fresh instance of `def_id` in `owner`'s hand.
    fn give(state: &mut GameState, def_id: &str, owner: PlayerId) -> String {
        let instance_id = format!("{def_id}@hand:{}", owner.as_str());
        let mut inst = CardInstance::new(instance_id.clone(), def_id.to_string(), owner, 0);
        inst.zone = Zone::Hand;
        state.cards.insert(instance_id.clone(), inst);
        state.players.get_mut(owner).hand.push(instance_id.clone());
        instance_id
    }

    fn last_log(state: &GameState) -> &str {
        &state.log.last().unwrap().message
    }

    // --- getAttackerOwner ---

    #[test]
    fn attacker_owner_never_throws_for_a_synthetic_captain_id() {
        let (state, _reg) = setup();
        assert_eq!(
            get_attacker_owner(&state, "captain_player1"),
            Ok(PlayerId::Player1)
        );
        assert_eq!(
            get_attacker_owner(&state, "captain_player2"),
            Ok(PlayerId::Player2)
        );
        // TS `attackerId.replace("captain_", "") as PlayerId` is unchecked: a
        // bogus suffix is not "player1", so `getOpponent` resolves it to
        // player1 and resolution simply continues — no throw.
        let bogus = get_attacker_owner(&state, "captain_nobody").unwrap();
        assert_eq!(bogus.opponent(), PlayerId::Player1);
        // Only the non-captain branch throws.
        assert_eq!(
            get_attacker_owner(&state, "ghost").unwrap_err().to_string(),
            "Attacker not found: ghost"
        );
    }

    // --- survivePlayed ---

    #[test]
    fn survive_played_is_a_real_pending_attack_field_named_survive_played() {
        let (mut state, _reg) = setup();
        assert!(!survive_played(&state));
        state.pending_attack = Some(PendingAttack {
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
        });
        assert!(!survive_played(&state));
        set_survive_played(&mut state, true);
        assert!(survive_played(&state));
        // The flag rides in its own field, not in `pushbackSlots`.
        assert_eq!(state.pending_attack.as_ref().unwrap().pushback_slots, None);
        let json = serde_json::to_string(state.pending_attack.as_ref().unwrap()).unwrap();
        assert!(json.contains("\"survivePlayed\":true"), "{json}");
        assert!(!json.contains("pushbackSlots"), "{json}");
        // A TS-produced pending attack deserialises the flag back.
        let round: PendingAttack = serde_json::from_str(&json).unwrap();
        assert_eq!(round.survive_played, Some(true));

        set_survive_played(&mut state, false);
        assert!(!survive_played(&state));
    }

    // --- declare* atomicity ---

    #[test]
    fn a_rejected_declaration_leaves_the_state_untouched() {
        let (mut state, reg) = setup();
        let attacker = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let before = state.clone();

        // `targetDisplayName` fails on an unknown target id: TS builds a new
        // state and throws before returning it, so nothing is committed.
        let err = declare_base_attack(&mut state, &reg, &attacker, "ghost", false).unwrap_err();
        assert_eq!(err.to_string(), "Instance not found: ghost");
        assert_eq!(state, before);
        assert!(!state.card(&attacker).unwrap().tapped);
        assert!(!state.card(&attacker).unwrap().used_base_action);
        assert!(!state.card(&attacker).unwrap().used_special_attack);
        assert!(state.pending_attack.is_none());
    }

    // --------------------------------------------------------

    #[test]
    fn base_attack_uses_both_actions_and_logs_the_ts_line() {
        let (mut state, reg) = setup();
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-DEF", PlayerId::Player2, Slot::V1);

        declare_base_attack(&mut state, &reg, &atk, &tgt, false).unwrap();

        // ATK 6 vs DEF 5 = 1
        assert_eq!(
            last_log(&state),
            "Attaquant attaque Defenseur (ATK 6 vs DEF 5 = 1 degats)"
        );
        let pending = state.pending_attack.clone().unwrap();
        assert_eq!(pending.raw_damage, 1);
        assert_eq!(pending.attack_power, Some(6));
        assert!(!pending.is_special);
        let a = state.card(&atk).unwrap();
        // One action per turn: BOTH flags are set by a base attack.
        assert!(a.tapped && a.used_base_action && a.used_special_attack);
    }

    #[test]
    fn piercing_halves_the_target_def_with_floor() {
        let (mut state, reg) = setup();
        let mut piercer = character("C-PIERCE", "Perceur", 6, 0, 8);
        piercer.traits = Some(vec![Trait::Piercing]);
        let mut reg = reg;
        reg.register_card(piercer);
        let atk = place(&mut state, &reg, "C-PIERCE", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-DEF", PlayerId::Player2, Slot::V1);

        declare_base_attack(&mut state, &reg, &atk, &tgt, false).unwrap();

        // DEF 5 -> floor(5/2) = 2, so 6 - 2 = 4
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 4);
        assert!(last_log(&state).contains("(ATK 6 vs DEF 2 = 4 degats)"));
    }

    #[test]
    fn trap_kills_the_attacker_before_any_pending_attack() {
        let (mut state, reg) = setup();
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-DEF", PlayerId::Player2, Slot::V1);
        {
            let a = state.card_mut(&atk).unwrap();
            a.current_pv = 2;
            a.status_effects.push(StatusEffect {
                effect_type: StatusEffectType::Trap,
                turns_remaining: 1,
                damage_per_turn: 3,
                source: "t".into(),
            });
        }

        declare_base_attack(&mut state, &reg, &atk, &tgt, false).unwrap();

        assert!(state.pending_attack.is_none());
        assert_eq!(
            state.log[0].message,
            "Piege ! Attaquant subit 3 degats en attaquant !"
        );
        assert_eq!(state.log[1].message, "Attaquant est KO par le piege !");
        assert_eq!(state.card(&atk).unwrap().zone, Zone::Graveyard);
    }

    #[test]
    fn counter_reduce_uses_the_captain_bonus_against_a_captain() {
        let (mut state, reg) = setup();
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let c = give(&mut state, "X-RED", PlayerId::Player2);
        declare_base_attack(&mut state, &reg, &atk, "captain", true).unwrap();
        // ATK 6 vs captain DEF 1 = 5
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 5);

        apply_counter_reduce(&mut state, &reg, &c).unwrap();

        // captainBonus 4 replaces amount 2
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 1);
        assert_eq!(last_log(&state), "Joue Esquive : reduit les degats de 4");
        assert!(state.players.player2.hand.is_empty());
        assert_eq!(state.card(&c).unwrap().zone, Zone::Graveyard);
    }

    #[test]
    fn shield_block_retargets_and_recomputes_the_damage() {
        let (mut state, reg) = setup();
        let mut blocker = character("C-SHIELD", "Bouclier", 1, 2, 9);
        blocker.traits = Some(vec![Trait::Shield]);
        blocker.passive = Some(passive(
            "Garde",
            vec![PassiveEffect::BlockDamageReduction { amount: 1 }],
        ));
        let mut reg = reg;
        reg.register_card(blocker);

        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let blk = place(&mut state, &reg, "C-SHIELD", PlayerId::Player2, Slot::V2);

        declare_base_attack(&mut state, &reg, &atk, &tgt, false).unwrap();
        apply_shield_block(&mut state, &reg, &blk).unwrap();

        let pending = state.pending_attack.clone().unwrap();
        assert_eq!(pending.target_id, blk);
        assert!(!pending.target_is_captain);
        // attackPower 6 - DEF 2 - blockDamageReduction 1 = 3
        assert_eq!(pending.raw_damage, 3);
        assert!(state.card(&blk).unwrap().tapped);
        assert_eq!(last_log(&state), "🛡 Bouclier bloque l'attaque ! (Bouclier)");
    }

    #[test]
    fn survive_counter_keeps_the_target_at_one_pv_only_for_that_attack() {
        let (mut state, reg) = setup();
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let c = give(&mut state, "X-SURV", PlayerId::Player2);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &atk, &tgt, false).unwrap();
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 6);
        apply_counter_survive(&mut state, &reg, &c).unwrap();
        assert!(survive_played(&state));
        assert_eq!(last_log(&state), "Joue Survie : survie a 1 PV !");
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        assert_eq!(state.card(&tgt).unwrap().current_pv, 1);
        assert_eq!(state.card(&tgt).unwrap().zone, Zone::Board);
        assert!(state.pending_attack.is_none());

        // A second attack (no counter) must NOT be saved again: the flag lived
        // on the pending attack, not on the character.
        state.card_mut(&atk).unwrap().tapped = false;
        state.card_mut(&atk).unwrap().used_base_action = false;
        declare_base_attack(&mut state, &reg, &atk, &tgt, false).unwrap();
        assert!(!survive_played(&state));
        resolve_attack(&mut state, &reg, &ctx).unwrap();
        assert_eq!(state.card(&tgt).unwrap().zone, Zone::Graveyard);
    }

    #[test]
    fn strawhat_saves_the_bearer_once() {
        let (mut state, reg) = setup();
        let mut reg = reg;
        reg.register_card(CardDef::new(
            "RH-014",
            "Chapeau de Paille",
            CardType::Object,
            1,
            Faction::Pirate,
            Rarity::R,
            "TEST",
        ));
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let hat = give(&mut state, "RH-014", PlayerId::Player2);
        state.card_mut(&tgt).unwrap().attached_objects.push(hat);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &atk, &tgt, false).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        assert_eq!(state.card(&tgt).unwrap().current_pv, 1);
        assert_eq!(
            last_log(&state),
            "Voisin survit grâce au Chapeau de Paille (1 PV) !"
        );
    }

    #[test]
    fn thunder_propagates_to_the_first_occupied_adjacent_slot_only() {
        let (mut state, reg) = setup();
        let mut thunder = character("C-THUNDER", "Foudre", 6, 0, 8);
        thunder.base_action = Some(BaseAction {
            name: "Eclair".to_string(),
            atk: 6,
            element: Some(Element::Thunder),
            ..Default::default()
        });
        let mut reg = reg;
        reg.register_card(thunder);

        let atk = place(&mut state, &reg, "C-THUNDER", PlayerId::Player1, Slot::V1);
        // Target V2 — adjacency is [V1, V3, A2]; V1 and V3 are both occupied,
        // only the first one may take the propagation.
        let tgt = place(&mut state, &reg, "C-DEF", PlayerId::Player2, Slot::V2);
        let first = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let second = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V3);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &atk, &tgt, false).unwrap();
        // 6 - 5 = 1 raw, propagation = max(1, floor(1/2)) = 1
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        assert_eq!(state.card(&first).unwrap().current_pv, 2);
        assert_eq!(state.card(&second).unwrap().current_pv, 3);
        assert_eq!(state.card(&tgt).unwrap().current_pv, 5);
    }

    #[test]
    fn zone_spread_skips_a_logia_secondary_without_haki() {
        let (mut state, reg) = setup();
        let mut zoner = character("C-ZONE", "Zone", 6, 0, 8);
        zoner.base_action = Some(BaseAction {
            name: "Vague".to_string(),
            atk: 6,
            attack_traits: Some(vec![AttackTrait::Zone]),
            ..Default::default()
        });
        let mut logia = character("C-LOGIA", "Logia", 1, 0, 4);
        logia.traits = Some(vec![Trait::Logia]);
        let mut reg = reg;
        reg.register_card(zoner);
        reg.register_card(logia);

        let atk = place(&mut state, &reg, "C-ZONE", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-DEF", PlayerId::Player2, Slot::V2);
        let normal = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let ghost = place(&mut state, &reg, "C-LOGIA", PlayerId::Player2, Slot::V3);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &atk, &tgt, false).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        // V1 (3 PV) is hit for 6 and KO'd, V3 (Logia) is untouched.
        assert_eq!(state.card(&normal).unwrap().zone, Zone::Graveyard);
        assert_eq!(state.card(&ghost).unwrap().current_pv, 4);
        assert_eq!(state.card(&ghost).unwrap().zone, Zone::Board);
        assert!(
            state
                .log
                .iter()
                .any(|l| l.message == "Voisin subit 6 degats (Zone/Total) (PV: -3)")
        );
    }

    #[test]
    fn logia_target_ignores_the_first_hit_of_the_turn_without_haki() {
        let (mut state, reg) = setup();
        let mut logia = character("C-LOGIA", "Logia", 1, 0, 4);
        logia.traits = Some(vec![Trait::Logia]);
        let mut reg = reg;
        reg.register_card(logia);
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-LOGIA", PlayerId::Player2, Slot::V1);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &atk, &tgt, false).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        assert_eq!(state.card(&tgt).unwrap().current_pv, 4);
        assert_eq!(state.card(&tgt).unwrap().logia_used_this_turn, Some(true));
        assert!(state.log.iter().any(
            |l| l.message == "⚠ Logia : INTANGIBILITE LOGIA ! Utilisez le Haki (T7+) ou l'Eau."
        ));
    }

    #[test]
    fn melee_recoil_hurts_a_non_range_attacker() {
        let (mut state, reg) = setup();
        let mut thorns = character("C-THORNS", "Epines", 1, 0, 9);
        thorns.passive = Some(passive(
            "Epines",
            vec![PassiveEffect::MeleeRecoil { amount: 2 }],
        ));
        let mut reg = reg;
        reg.register_card(thorns);
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-THORNS", PlayerId::Player2, Slot::V1);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &atk, &tgt, false).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        assert_eq!(state.card(&atk).unwrap().current_pv, 6);
        assert!(
            state
                .log
                .iter()
                .any(|l| l.message == "Attaquant subit 2 dégâts (Épines) !")
        );
    }

    #[test]
    fn eligible_counters_filter_on_max_attacker_atk() {
        let (mut state, reg) = setup();
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let red = give(&mut state, "X-RED", PlayerId::Player2);
        let cancel = give(&mut state, "X-CANCEL", PlayerId::Player2);

        // No pending attack yet.
        assert!(
            get_eligible_counters(&state, &reg, PlayerId::Player2)
                .unwrap()
                .is_empty()
        );

        declare_base_attack(&mut state, &reg, &atk, &tgt, false).unwrap();
        // attackPower 6 > maxAttackerAtk 4 → the cancel counter drops out.
        assert_eq!(
            get_eligible_counters(&state, &reg, PlayerId::Player2).unwrap(),
            vec![red.clone()]
        );
        assert_eq!(
            apply_counter_cancel(&mut state, &reg, &cancel).unwrap_err(),
            EngineError::illegal("Attacker is too strong for this counter")
        );

        // Weaken the attack: both counters become eligible.
        state.pending_attack.as_mut().unwrap().attack_power = Some(3);
        assert_eq!(
            get_eligible_counters(&state, &reg, PlayerId::Player2).unwrap(),
            vec![red, cancel.clone()]
        );
        apply_counter_cancel(&mut state, &reg, &cancel).unwrap();
        assert!(state.pending_attack.is_none());
        assert_eq!(last_log(&state), "Faible : attaque annulée !");
    }

    #[test]
    fn flipped_logia_captain_ignores_a_hakiless_hit() {
        let (mut state, reg) = setup();
        state.players.player2.captain = CaptainInstance::new("CAP-L".into(), PlayerId::Player2, 20);
        state.players.player2.captain.flipped = true;
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &atk, "captain", true).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        assert_eq!(state.players.player2.captain.current_pv, 20);
        assert!(state.log.iter().any(|l| l.message
            == "⚠ Logia : INTANGIBILITE LOGIA ! L'attaque passe a travers. Utilisez le Haki (T7+) ou l'Eau pour le toucher."));

        // Turn 7+ grants Armament Haki: the same attack now lands.
        state.turn_number = 7;
        state.card_mut(&atk).unwrap().tapped = false;
        state.card_mut(&atk).unwrap().used_base_action = false;
        declare_base_attack(&mut state, &reg, &atk, "captain", true).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();
        assert_eq!(state.players.player2.captain.current_pv, 15);
        assert_eq!(last_log(&state), "Capitaine Logia subit 5 degats (PV: 15)");
    }

    #[test]
    fn captain_attacker_ids_round_trip() {
        let (state, _reg) = setup();
        assert_eq!(captain_attacker_id(PlayerId::Player2), "captain_player2");
        assert_eq!(
            get_attacker_owner(&state, "captain_player2").unwrap(),
            PlayerId::Player2
        );
        assert_eq!(
            get_attacker_owner(&state, "nope").unwrap_err(),
            EngineError::illegal("Attacker not found: nope")
        );
    }
}

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
    granted_attack_traits, has_summoning_sickness, has_trait, heal_unit, remove_from_board,
};
use crate::captain::captain_has_trait;
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::passives::apply_on_ko_effects;
use crate::registry::CardRegistry;
use crate::state::{GameState, ONCE_STRAWHAT, ONCE_SURVIVED, PendingAttack, transactional};
use crate::types::{
    AttackTrait, CardType, ConditionalBonus, CounterEffect, Element, HakiType, Modifier,
    ModifierDuration, ModifierStat, PassiveEffect, PlayerId, Slot, SpecialAttack, StatusEffect,
    StatusEffectType, Trait, Zone,
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
        // Decision §8.37 (`betrayal`): `controlledBy` first, `owner` after —
        // a borrowed attacker fights for its borrower.
        Some(card) => Ok(card.controller()),
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

/// Decision §8.23 — the one meaning given to `Modifier.stat == "haki"`.
///
/// A modifier with `stat == Haki` and `amount > 0` living on the attacker
/// (a board character *or* the captain behind a synthetic `captain_{player}`
/// id) makes `hasHaki` true for the attacks it declares while the modifier
/// lasts — the per-instance counterpart of the player-wide `hakiThisTurn`.
/// Nothing in the shipped catalogue emits such a modifier.
pub fn attacker_haki_modifier(state: &GameState, attacker_instance_id: &str) -> bool {
    let mods = if let Some(rest) = attacker_instance_id.strip_prefix(CAPTAIN_ATTACKER_PREFIX) {
        let owner = match rest {
            "player1" => PlayerId::Player1,
            _ => PlayerId::Player2,
        };
        &state.players.get(owner).captain.modifiers
    } else {
        match state.cards.get(attacker_instance_id) {
            Some(card) => &card.modifiers,
            None => return false,
        }
    };
    mods.iter()
        .any(|m| m.stat == ModifierStat::Haki && m.amount > 0)
}

/// Decision §8.13 — does this pending attack halve its target's DEF?
///
/// The keyword can sit on the attack (`attackTraits: ["piercing"]`) *or* on
/// the attacker itself (`CardDef.traits`, or the captain's active face); the
/// declaration honours both, so [`apply_shield_block`] must too.
fn pending_is_piercing(
    state: &GameState,
    registry: &CardRegistry,
    pending: &PendingAttack,
) -> Result<bool, EngineError> {
    if pending.attack_traits.contains(&AttackTrait::Piercing) {
        return Ok(true);
    }
    if let Some(rest) = pending.attacker_id.strip_prefix(CAPTAIN_ATTACKER_PREFIX) {
        let owner = match rest {
            "player1" => PlayerId::Player1,
            _ => PlayerId::Player2,
        };
        let cap = &state.players.get(owner).captain;
        let cap_def = registry.get_captain_def(&cap.def_id)?;
        return Ok(captain_has_trait(cap_def, cap.flipped, Trait::Piercing));
    }
    let Some(card) = state.cards.get(&pending.attacker_id) else {
        return Ok(false);
    };
    Ok(registry.get_card_def(&card.def_id)?.has_trait(Trait::Piercing))
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
    // Decision §8.55: DEF is a non-negative stat. `get_effective_def` already
    // clamps a character's; the captain's face DEF plus modifiers now does the
    // same, so a debuffed captain can never *gain* damage through the piercing
    // `floor(def / 2)` rounding toward minus infinity.
    Ok(target_def.max(0))
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
    ctx: &EngineContext,
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
        let ko_def_id = state.get_card(attacker_instance_id)?.def_id.clone();
        state.add_log(owner, format!("{def_name} est KO par le piege !"));
        // Decision §8.12: a trap KO is a KO like any other — the +2 Volonte
        // goes to the player who lost the unit (Rulebook v3.1 §4) and the
        // on-KO triggers fire, with the trapper's side as the killer.
        state.grant_ko_bonus(owner);
        remove_from_board(state, registry, attacker_instance_id)?;
        apply_on_ko_effects(state, registry, ctx, owner, owner.opponent(), &ko_def_id)?;
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
    ctx: &EngineContext,
    attacker_instance_id: &str,
    target_instance_id: &str,
    target_is_captain: bool,
) -> Result<(), EngineError> {
    transactional(state, |state| {
        declare_base_attack_inner(
            state,
            registry,
            ctx,
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
    ctx: &EngineContext,
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
    // Decision §8.37 (`betrayal`): a borrowed body attacks for whoever
    // controls it, so its side of the board is the controller's.
    let owner = attacker.controller();
    let attached: Vec<String> = attacker.attached_objects.clone();
    let def = registry.get_card_def(&attacker.def_id)?;
    let def_name = def.name.clone();
    let def_piercing = def.has_trait(Trait::Piercing);
    let def_has_natural_haki = crate::haki::def_has_natural_haki(def);
    let base_action = def.base_action.clone();

    if get_effective_atk(state, registry, attacker_instance_id)? <= 0 {
        return Err(EngineError::illegal("Character has 0 ATK — cannot attack"));
    }

    // Trigger trap on attacker if present
    if trigger_attacker_trap(state, registry, ctx, attacker_instance_id, owner, &def_name)? {
        return Ok(());
    }

    let atk = get_effective_atk(state, registry, attacker_instance_id)?;
    let mut attack_traits: Vec<AttackTrait> = base_action
        .as_ref()
        .and_then(|b| b.attack_traits.clone())
        .unwrap_or_default();
    // Decision §8.47: the `AttackTrait` arm of an equipped object's
    // `grantsTraits` joins every attack the bearer declares.
    for at in granted_attack_traits(state, registry, attacker_instance_id)? {
        if !attack_traits.contains(&at) {
            attack_traits.push(at);
        }
    }

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

    // Apply Piercing (DEF / 2) — decision §8.55: never on a negative DEF.
    let is_piercing = attack_traits.contains(&AttackTrait::Piercing) || def_piercing;
    if is_piercing {
        target_def = target_def.max(0).div_euclid(2);
    }

    let raw_damage = (atk - target_def).max(0);

    // Haki to pierce Logia: natural Haki, Armament passive (T7+), Water
    // element, the player-wide `hakiThisTurn` or — decision §8.23 — a `haki`
    // modifier on the attacker itself.
    let has_haki = def_has_natural_haki
        || state.turn_number >= 7
        || attack_element == Some(Element::Water)
        || state.players.get(owner).has_haki_this_turn()
        || attacker_haki_modifier(state, attacker_instance_id);

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
        // Decision §8.38: `BaseAction.cannotBeDodged` is finally *read* (no
        // shipped base action sets it, so this is inert on the catalogue).
        cannot_be_dodged: Some(
            base_action
                .as_ref()
                .and_then(|b| b.cannot_be_dodged)
                .unwrap_or(false)
                || attacker_no_dodge(state, registry, attacker_instance_id)?,
        ),
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
        survive_target_id: None,
        damage_reduction: None,
        ignore_def: None,
        permanent_pv_loss: None,
        no_heal: None,
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
    // Decision §8.37 (`betrayal`): a borrowed body attacks for whoever
    // controls it, so its side of the board is the controller's.
    let owner = attacker.controller();
    let def = registry.get_card_def(&attacker.def_id)?;
    let def_name = def.name.clone();
    let def_piercing = def.has_trait(Trait::Piercing);
    let def_has_natural_haki = crate::haki::def_has_natural_haki(def);
    let spec = def.special_attack.clone();

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

    // Decision §8.12: the trap fires only once the declaration is legal.
    // Firing it before the checks let a refused declaration eat the trap —
    // and the transactional wrapper then rolled the consumption back, so the
    // trap was silently free.
    if trigger_attacker_trap(state, registry, ctx, attacker_instance_id, owner, &def_name)? {
        return Ok(());
    }

    // Self-transformation special (Chopper Monster Point): no target, no pending attack.
    if let Some(t) = spec.transform {
        state.spend_volonte(owner, spec.cost)?;
        let cur_atk = get_effective_atk(state, registry, attacker_instance_id)?;
        let cur_def = get_effective_def(state, registry, attacker_instance_id)?;
        {
            let c = state.get_card_mut(attacker_instance_id)?;
            c.used_base_action = true;
            c.used_special_attack = true;
            c.tapped = true;
            if spec.once_per_game.unwrap_or(false) {
                c.used_once_abilities.push(spec.name.clone());
            }
            // Decision §8.39: the whole Transform, not just its ATK.
            // "ATK 8 - DEF 2 - PV 6 + Rush pendant 2 tours, puis KO
            // automatique" (MG-008 Monster Point) is five clauses; the ATK
            // delta may be negative (a transform into a weaker form lowers it).
            c.modifiers.push(Modifier {
                id: format!("transform_{}_{}", spec.name, ctx.now()),
                stat: ModifierStat::Atk,
                amount: t.atk - cur_atk,
                source: "transform".to_string(),
                duration: ModifierDuration::Permanent,
                turns_remaining: None,
            });
            c.modifiers.push(Modifier {
                id: format!("transform_def_{}_{}", spec.name, ctx.now()),
                stat: ModifierStat::Def,
                amount: t.def - cur_def,
                source: "transform".to_string(),
                duration: ModifierDuration::Permanent,
                turns_remaining: None,
            });
            c.current_pv = t.pv;
            // Rush for the duration: the same `-1` sentinel `grantSelfRush`
            // writes, so the transformed unit may act at once.
            c.deployed_turn = Some(-1);
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

    // Decision §8.38: a *support* special resolves its structured fields and
    // never builds a pending attack — there is nothing for the defender to
    // counter, block or dodge.
    if spec.is_support.unwrap_or(false) {
        return resolve_support_special(
            state,
            registry,
            ctx,
            owner,
            attacker_instance_id,
            &def_name,
            &spec,
            target_instance_id,
        );
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
    // Decision §8.47: equipment-granted attack traits join this attack too.
    for at in granted_attack_traits(state, registry, attacker_instance_id)? {
        if !attack_traits.contains(&at) {
            attack_traits.push(at);
        }
    }

    let mut target_def_val = target_def_value(
        state,
        registry,
        owner,
        target_instance_id,
        target_is_captain,
    )?;

    // Piercing — decision §8.55: never applied to a negative DEF.
    let is_piercing = attack_traits.contains(&AttackTrait::Piercing) || def_piercing;
    if is_piercing {
        target_def_val = target_def_val.max(0).div_euclid(2);
    }

    // Ignore DEF (TS truthiness: `ignoreDef: 0` is falsy — no clamp at all)
    if let Some(ignore) = spec.ignore_def.filter(|v| *v != 0) {
        target_def_val = (target_def_val - ignore).max(0);
    }

    let raw_damage = (total_atk - target_def_val).max(0);

    // Haki to pierce Logia: natural Haki, Armament passive (T7+), Water
    // element, `hakiThisTurn`, or the §8.23 `haki` modifier on the attacker.
    let has_haki = def_has_natural_haki
        || state.turn_number >= 7
        || spec.element == Some(Element::Water)
        || state.players.get(owner).has_haki_this_turn()
        || attacker_haki_modifier(state, attacker_instance_id);

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
        // Decision §8.38: the board has exactly two rows, so any
        // `pushbackSlots >= 1` is one pushback — the same knock-back the
        // `pushback` flag and the `impact` keyword produce.
        pushback: Some(spec.pushback.unwrap_or(false) || spec.pushback_slots.unwrap_or(0) > 0),
        pushback_slots: None,
        strip_stealth: Some(spec.strip_stealth.unwrap_or(false) || strips_stealth),
        survive_played: None,
        survive_target_id: None,
        damage_reduction: None,
        ignore_def: spec.ignore_def,
        permanent_pv_loss: spec.permanent_pv_loss,
        no_heal: spec.no_heal,
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

/// Decision §8.38 — resolve a support special (`SpecialAttack.isSupport`).
///
/// "Support" is the printed opposite of an attack: `RH-004` Provocation,
/// `BW-005` Peinture de la Colère and `RH-009` Stimulant deal no damage, so
/// they spend the cost, spend the attacker's action and resolve their
/// structured fields on the chosen unit — **without** parking a
/// [`PendingAttack`], because there is no blow for the defender to counter,
/// block or dodge.
///
/// The fields honoured are `healAmount`, `buffAllyAtk`, `cleanse`, `taunt`,
/// `immobilize`, `sleep` and `stripStealth` (decision §8.54: structured card
/// data, never description parsing). `immuneControl` still absorbs the two
/// control effects, exactly as it does on a landed attack.
///
/// Errors: `Support special needs a target` when a field that needs one is
/// present and `targetInstanceId` is not a character on the board.
#[allow(clippy::too_many_arguments)]
fn resolve_support_special(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    owner: PlayerId,
    attacker_instance_id: &str,
    def_name: &str,
    spec: &SpecialAttack,
    target_instance_id: &str,
) -> Result<(), EngineError> {
    let heal = spec.heal_amount.unwrap_or(0);
    let buff = spec.buff_ally_atk.unwrap_or(0);
    let cleanse = spec.cleanse.unwrap_or(false);
    let taunt = spec.taunt.unwrap_or(false);
    let immobilize = spec.immobilize.unwrap_or(false);
    let sleep = spec.sleep.unwrap_or(false);
    let strip_stealth = spec.strip_stealth.unwrap_or(false);

    let needs_target =
        heal != 0 || buff != 0 || cleanse || taunt || immobilize || sleep || strip_stealth;
    let target_on_board = state
        .cards
        .get(target_instance_id)
        .is_some_and(|c| c.zone == Zone::Board);
    if needs_target && !target_on_board {
        return Err(EngineError::illegal("Support special needs a target"));
    }

    state.spend_volonte(owner, spec.cost)?;
    {
        let a = state.get_card_mut(attacker_instance_id)?;
        a.tapped = true;
        // One action per turn (Rulebook v3.1 §2.2/§6).
        a.used_base_action = true;
        a.used_special_attack = true;
        if spec.once_per_game.unwrap_or(false) {
            a.used_once_abilities.push(spec.name.clone());
        }
    }

    if !needs_target {
        state.add_log(owner, format!("{def_name} utilise {} !", spec.name));
        return Ok(());
    }

    let target_def_id = state.get_card(target_instance_id)?.def_id.clone();
    let target_name = registry.get_card_def(&target_def_id)?.name.clone();
    let ctrl_immune = registry
        .get_card_def(&target_def_id)?
        .passive
        .as_ref()
        .is_some_and(|p| {
            p.effects
                .iter()
                .any(|e| matches!(e, PassiveEffect::ImmuneControl))
        });

    if heal != 0 {
        // Decision §8.5: the single heal path (never lowers PV, honours
        // `noHeal` / `desiccation` and the permanent max-PV loss).
        let healed = heal_unit(state, registry, target_instance_id, heal)?;
        state.add_log(
            owner,
            format!(
                "{def_name} utilise {} : +{healed} PV a {target_name}",
                spec.name
            ),
        );
    }
    if buff != 0 {
        state
            .get_card_mut(target_instance_id)?
            .modifiers
            .push(Modifier {
                id: format!("support_{}_{}", spec.name, ctx.now()),
                stat: ModifierStat::Atk,
                amount: buff,
                source: format!("support_{}", spec.name),
                duration: ModifierDuration::Turn,
                turns_remaining: None,
            });
        state.add_log(
            owner,
            format!(
                "{def_name} utilise {} : {target_name} +{buff} ATK ce tour.",
                spec.name
            ),
        );
    }
    if cleanse {
        state
            .get_card_mut(target_instance_id)?
            .status_effects
            .retain(|e| {
                !matches!(
                    e.effect_type,
                    StatusEffectType::Freeze
                        | StatusEffectType::Immobilize
                        | StatusEffectType::Sleep
                        | StatusEffectType::LoseAction
                )
            });
        state.add_log(
            owner,
            format!(
                "{def_name} utilise {} : {target_name} perd gelé/immobilisé !",
                spec.name
            ),
        );
    }
    if taunt {
        state
            .get_card_mut(target_instance_id)?
            .status_effects
            .push(StatusEffect {
                effect_type: StatusEffectType::Taunt,
                // 2 so it survives the start-of-turn decrement and actually
                // binds the target's *next* turn.
                turns_remaining: 2,
                damage_per_turn: 0,
                source: attacker_instance_id.to_string(),
            });
        state.add_log(
            owner,
            format!(
                "{def_name} utilise {} : {target_name} doit cibler {def_name} a son prochain tour !",
                spec.name
            ),
        );
    }
    if immobilize && !ctrl_immune {
        state
            .get_card_mut(target_instance_id)?
            .status_effects
            .push(StatusEffect {
                effect_type: StatusEffectType::Immobilize,
                turns_remaining: 2,
                damage_per_turn: 0,
                source: attacker_instance_id.to_string(),
            });
        state.add_log(owner, format!("{target_name} est immobilisé !"));
    }
    if sleep && !ctrl_immune {
        state
            .get_card_mut(target_instance_id)?
            .status_effects
            .push(StatusEffect {
                effect_type: StatusEffectType::Sleep,
                turns_remaining: 3,
                damage_per_turn: 0,
                source: attacker_instance_id.to_string(),
            });
        state.add_log(owner, format!("{target_name} est endormi !"));
    }
    if strip_stealth {
        state
            .get_card_mut(target_instance_id)?
            .status_effects
            .push(StatusEffect {
                effect_type: StatusEffectType::NoStealth,
                turns_remaining: 2,
                damage_per_turn: 0,
                source: attacker_instance_id.to_string(),
            });
    }

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
    // Decision §8.37 (`betrayal`): a borrowed body attacks for whoever
    // controls it, so its side of the board is the controller's.
    let owner = attacker.controller();
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
    let def_has_natural_haki = crate::haki::def_has_natural_haki(def);

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
        // Decision §8.55: DEF is clamped at 0 before the halving.
        target_def_val = target_def_val.max(0).div_euclid(2);
    }
    if let Some(ignore) = spec.ignore_def.filter(|v| *v != 0) {
        target_def_val = (target_def_val - ignore).max(0);
    }

    let raw_damage = (total_atk - target_def_val).max(0);

    // NB (TS quirk, kept): the fruit path omits the player-wide
    // `hakiThisTurn`. The §8.23 per-instance `haki` modifier is read here too.
    let has_haki = def_has_natural_haki
        || state.turn_number >= 7
        || spec.element == Some(Element::Water)
        || attacker_haki_modifier(state, attacker_instance_id);

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
        survive_target_id: None,
        damage_reduction: None,
        ignore_def: spec.ignore_def,
        // §8.48: the awakening special is a *different, smaller* shape than
        // `SpecialAttack` — it has no `permanentPvLoss` / `noHeal` /
        // `pushbackSlots` field at all, so there is nothing to carry here.
        permanent_pv_loss: None,
        no_heal: None,
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
    // Decision §8.14: `captainBonus` *adds* to `amount` against a captain —
    // the field is a bonus, as its name says, not a replacement total — and a
    // `Some(0)` is a real value (the JS-falsy-zero quirk is gone; adding 0
    // simply changes nothing). No shipped counter carries the field.
    let targets_captain = state
        .pending_attack
        .as_ref()
        .is_some_and(|p| p.target_is_captain);
    if targets_captain {
        reduction += captain_bonus.unwrap_or(0);
    }

    state.spend_volonte(owner, counter_cost)?;

    discard_counter(state, owner, counter_instance_id);
    if let Some(pending) = state.pending_attack.as_mut() {
        pending.raw_damage = (pending.raw_damage - reduction).max(0);
        // Decision §8.13: remember what the defender paid for, so a later
        // Bouclier block cannot silently refund it.
        pending.damage_reduction =
            Some(pending.damage_reduction.unwrap_or(0) + reduction.max(0));
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
        .is_some_and(|c| c.used_once(ONCE_SURVIVED))
    {
        return Err(EngineError::illegal("This character already survived once"));
    }

    state.spend_volonte(owner, counter_cost)?;

    discard_counter(state, owner, counter_instance_id);
    // Decision §8.15: the counter only *records* which ally it protects. The
    // 1-PV floor and the once-per-character `"survived"` tag are applied by
    // `applyCharacterDamage`, and only if the attack still lands on that
    // ally — a cancelled attack or a Bouclier block must not burn the tag.
    set_survive_played(state, true);
    if let Some(pending) = state.pending_attack.as_mut() {
        pending.survive_target_id = Some(protected_id.clone());
    }

    state.add_log(owner, format!("Joue {counter_name} : survie a 1 PV !"));
    Ok(())
}

/// TS `applyShieldBlock(state, blockerInstanceId)` — `src/engine/combat.ts:603`.
///
/// The Bouclier reaction: taps the blocker, retargets the pending attack at it
/// and recomputes the damage against the blocker's own DEF. Logs
/// `"🛡 {name} bloque l'attaque ! (Bouclier)"`.
///
/// Decision §8.13 — the TS original validated almost nothing. The printed rule
/// is "S'incline pour bloquer pour 1 adjacent", so the blocker must be a
/// character **on the board**, owned by the **defender**, **not** the unit
/// already targeted, **untapped**, carrying `Trait::Shield` (through
/// [`has_trait`], so a fruit- or equipment-granted Bouclier counts) and
/// **adjacent** to the target's slot — the captain's slot included, since a
/// flipped captain stands in one.
///
/// The recomputation is `max(0, attackPower − blockerDef − blockDamageReduction)
/// − damageReduction`, where the blocker's DEF is halved by the attack's *or*
/// the attacker's own Piercing and lowered by the special's `ignoreDef`
/// (both of which the declaration had applied and the TS block dropped), and
/// `damageReduction` re-applies every `reduceDamage` counter the defender
/// already paid for.
///
/// Errors: `No pending attack`, `This attack ignores Bouclier`,
/// `Blocker not found`, `Blocker is not on the board`,
/// `Blocker is not yours`, `The target cannot block for itself`,
/// `A tapped character cannot use Bouclier`, `Not a Bouclier character`,
/// `Blocker is not adjacent to the target`.
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
    let blocker_owner = blocker.owner;
    let blocker_slot = blocker.slot;
    if blocker.zone != Zone::Board || blocker_slot.is_none() {
        return Err(EngineError::illegal("Blocker is not on the board"));
    }
    // The defender is the attacker's opponent — a Bouclier only ever blocks
    // for its own side.
    let defender_id = get_attacker_owner(state, &pending.attacker_id)?.opponent();
    if blocker_owner != defender_id {
        return Err(EngineError::illegal("Blocker is not yours"));
    }
    if blocker_instance_id == pending.target_id {
        return Err(EngineError::illegal("The target cannot block for itself"));
    }
    if blocker.tapped {
        return Err(EngineError::illegal(
            "A tapped character cannot use Bouclier",
        ));
    }
    if !has_trait(state, registry, blocker_instance_id, Trait::Shield)? {
        return Err(EngineError::illegal("Not a Bouclier character"));
    }

    // "pour 1 adjacent": the blocker stands next to whoever is being hit —
    // a character, or the captain in its own slot.
    let target_slot = if pending.target_is_captain {
        state.players.get(defender_id).captain.slot
    } else {
        state.cards.get(&pending.target_id).and_then(|c| c.slot)
    };
    let adjacent = match (blocker_slot, target_slot) {
        (Some(b), Some(t)) => b.is_adjacent_to(t),
        _ => false,
    };
    if !adjacent {
        return Err(EngineError::illegal(
            "Blocker is not adjacent to the target",
        ));
    }

    let mut blocker_def = get_effective_def(state, registry, blocker_instance_id)?;
    // Piercing from the attack *or* from the attacker itself, then the
    // special's `ignoreDef` — the two clamps the declaration had applied.
    if pending_is_piercing(state, registry, &pending)? {
        blocker_def = blocker_def.max(0).div_euclid(2);
    }
    if let Some(ignore) = pending.ignore_def.filter(|v| *v != 0) {
        blocker_def = (blocker_def - ignore).max(0);
    }
    // Some blockers reduce the damage further (Sentomaru, Garp).
    let blocker = state.get_card(blocker_instance_id)?;
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
    let new_raw = ((atk_power - blocker_def - block_red).max(0)
        - pending.damage_reduction.unwrap_or(0))
    .max(0);

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
        // Decision §8.16: no `survivePlayed` branch — `applyCounterSurvive`
        // refuses captain targets, so it could never be reached.
        apply_captain_damage(state, registry, &pending)?;
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

    // Apply element effects. Decision §8.18: they resolve from the *entry*
    // snapshot, so Foudre still propagates off a killing blow.
    apply_element_effects(state, registry, &pending, entry_target_slot)?;

    // Decision §8.38 — `permanentPvLoss`: "La cible perd 2 PV permanent
    // (Sable)" (Crocodile's Desert Girasol). The maximum drops for good and
    // the current PV follows it down, so a later heal can never climb back.
    if let Some(loss) = pending.permanent_pv_loss.filter(|n| *n > 0) {
        if pending.target_is_captain {
            let attacker_owner = get_attacker_owner(state, &pending.attacker_id)?;
            let opponent_id = attacker_owner.opponent();
            let cap_def_id = state.players.get(opponent_id).captain.def_id.clone();
            let cap_def = registry.get_captain_def(&cap_def_id)?;
            let cap = &mut state.players.get_mut(opponent_id).captain;
            cap.pv_max_loss = Some(cap.pv_max_loss.unwrap_or(0) + loss);
            let printed = if cap.flipped {
                cap_def.verso.pv
            } else {
                cap_def.recto.pv
            };
            let max_pv = printed - cap.pv_max_loss.unwrap_or(0);
            if cap.current_pv > max_pv {
                cap.current_pv = max_pv;
            }
        } else if state
            .cards
            .get(&pending.target_id)
            .is_some_and(|c| c.zone == Zone::Board)
        {
            {
                let t = state.get_card_mut(&pending.target_id)?;
                t.pv_max_loss = Some(t.pv_max_loss.unwrap_or(0) + loss);
            }
            if let Some(max_pv) = crate::board::max_pv_of(state, registry, &pending.target_id)? {
                let t = state.get_card_mut(&pending.target_id)?;
                if t.current_pv > max_pv {
                    t.current_pv = max_pv;
                }
            }
        }
    }

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
            // Decision §8.38 — `noHeal`: the target cannot be healed for 2
            // turns (`heal_unit` skips a unit carrying the status).
            if pending.no_heal.unwrap_or(false) {
                state
                    .get_card_mut(&pending.target_id)?
                    .status_effects
                    .push(StatusEffect {
                        effect_type: StatusEffectType::NoHeal,
                        turns_remaining: 2,
                        damage_per_turn: 0,
                        source: pending.attacker_id.clone(),
                    });
                state.add_log(tgt_owner, format!("{tdef_name} ne peut plus être soigné !"));
            }
            // Decision §8.38: the `impact` keyword *is* the pushback —
            // "Impact - repousse la cible d'un slot" — so Garp's Poing de
            // l'Amour, Sengoku's Daibutsu, Bonk Punch's Charge and Howling
            // Gab's Onde de Choc push even though they set no `pushback` flag.
            let pushes = pending.pushback.unwrap_or(false)
                || pending.attack_traits.contains(&AttackTrait::Impact);
            if pushes && !impact_immune {
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
        // Decision §8.55: DEF is clamped at 0 before the halving.
        target_def_val = target_def_val.max(0).div_euclid(2);
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
    let spread_slot = state.cards.get(target_id).and_then(|c| c.slot);
    apply_element_effects(state, registry, &spread_pending, spread_slot)?;

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
/// A Logia captain with no Haki on the attack ignores the hit entirely
/// (log `"⚠ {name} : INTANGIBILITE LOGIA ! L'attaque passe a travers. Utilisez
/// le Haki (T7+) ou l'Eau pour le toucher."`). Otherwise subtracts
/// `pending.rawDamage` and logs
/// `"Capitaine {name} subit {n} degats (PV: {pv})"`.
///
/// Decision §8.40 — the Logia keyword is read through
/// [`crate::captain::captain_has_trait`] (the card-level traits on both faces
/// plus `verso.traits` when flipped) and, exactly like a character's
/// (Rulebook §7), it protects **once per turn**: the second unhakied hit of
/// the turn goes through.
///
/// Decision §8.16 — there is no `survivePlayed` branch here:
/// [`apply_counter_survive`] refuses a captain target ("Survive only protects
/// allies"), so the branch was unreachable by construction.
pub fn apply_captain_damage(
    state: &mut GameState,
    registry: &CardRegistry,
    pending: &PendingAttack,
) -> Result<(), EngineError> {
    let attacker_owner = get_attacker_owner(state, &pending.attacker_id)?;
    let opponent_id = attacker_owner.opponent();
    let cap = &state.players.get(opponent_id).captain;
    let cap_flipped = cap.flipped;
    let logia_used_this_turn = cap.logia_used_this_turn.unwrap_or(false);
    let cap_def = registry.get_captain_def(&cap.def_id)?;
    let cap_name = cap_def.name.clone();

    // Logia check on captain — once per turn.
    if captain_has_trait(cap_def, cap_flipped, Trait::Logia)
        && !pending.has_haki
        && pending.raw_damage > 0
        && !logia_used_this_turn
    {
        state.players.get_mut(opponent_id).captain.logia_used_this_turn = Some(true);
        state.add_log(
            attacker_owner,
            format!(
                "⚠ {cap_name} : INTANGIBILITE LOGIA ! L'attaque passe a travers. Utilisez le Haki (T7+) ou l'Eau pour le toucher."
            ),
        );
        return Ok(());
    }

    let damage = pending.raw_damage;

    let target_cap = &mut state.players.get_mut(opponent_id).captain;
    target_cap.current_pv -= damage;
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

    // Decision §8.15: the survive counter protects the ally it named. A
    // Bouclier block retargets the attack, and the blocker is *not* that ally,
    // so it takes the blow normally. (A pending attack that came from the TS
    // engine carries `survivePlayed` without a target id — it then protects
    // whoever the attack resolves against, i.e. the old behaviour.)
    let protects_this_target = survive_played
        && pending
            .survive_target_id
            .as_deref()
            .is_none_or(|id| id == pending.target_id);
    {
        let t = state.get_card_mut(&pending.target_id)?;
        t.current_pv -= damage;
        if protects_this_target && t.current_pv <= 0 {
            t.current_pv = 1;
            // The once-per-character tag is spent only now, on an attack that
            // actually landed on the protected ally.
            if !t.used_once(ONCE_SURVIVED) {
                t.used_once_abilities.push(ONCE_SURVIVED.to_string());
            }
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
        let used_strawhat = bearer.used_once(ONCE_STRAWHAT);
        if has_strawhat && !used_strawhat {
            {
                let b = state.get_card_mut(&pending.target_id)?;
                b.current_pv = 1;
                b.used_once_abilities.push(ONCE_STRAWHAT.to_string());
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
///
/// Decision §8.18 — `entry_slot` is the target's slot **as the attack was
/// resolved**, threaded down from [`resolve_attack`]. Foudre is the
/// propagation of the blow that landed, so a lethal blow still splashes: the
/// thunder arm no longer needs the target to be alive on the board. The other
/// five elements are statuses or damage *on the target*, so they stay no-ops
/// once it has left the board.
///
/// The raw-vs-power split is deliberate and unchanged: thunder splashes half
/// of the damage actually **dealt** (`rawDamage`, after counters), while the
/// Zone/Total spread is a second full swing (`attackPower`).
pub fn apply_element_effects(
    state: &mut GameState,
    registry: &CardRegistry,
    pending: &PendingAttack,
    entry_slot: Option<Slot>,
) -> Result<(), EngineError> {
    let Some(element) = pending.element else {
        return Ok(());
    };
    if pending.target_is_captain {
        // Apply element to captain
        return apply_element_to_captain(state, registry, pending);
    }

    let on_board = state
        .cards
        .get(&pending.target_id)
        .is_some_and(|c| c.zone == Zone::Board);
    // The slot the blow landed on: the live one while the target is still
    // standing, the entry snapshot once the KO removal cleared it.
    let target_slot = state
        .cards
        .get(&pending.target_id)
        .and_then(|c| c.slot)
        .or(entry_slot);

    match element {
        // Propagate damage to 1 adjacent — decision §8.18: even off a kill.
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
            return Ok(());
        }
        _ if !on_board => return Ok(()),
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
/// Decision §8.40 — "simplified for MVP" is not a rule: all six elements now
/// land on a captain, with the meaning the element already has on a character:
///
/// * fire → `burn` 2t/1 dmg;
/// * ice → `freeze` 2t;
/// * poison → permanent `poison` 1 dmg (the start-of-turn tick already floors
///   a poisoned captain at 1 PV);
/// * thunder → `max(1, floor(raw/2))` to the first occupied slot adjacent to
///   the captain's own slot (nothing while the captain is off-board, recto);
/// * sand → one point of permanent maximum PV;
/// * water → the raw damage a second time when the captain's active traits
///   carry `cursed` — the Luffy, Akainu and Crocodile versos all do.
pub fn apply_element_to_captain(
    state: &mut GameState,
    registry: &CardRegistry,
    pending: &PendingAttack,
) -> Result<(), EngineError> {
    let attacker_owner = get_attacker_owner(state, &pending.attacker_id)?;
    let opponent_id = attacker_owner.opponent();

    let status = |t: StatusEffectType, turns: i32, dmg: i32| StatusEffect {
        effect_type: t,
        turns_remaining: turns,
        damage_per_turn: dmg,
        source: pending.attacker_id.clone(),
    };

    match pending.element {
        Some(Element::Fire) => {
            state
                .players
                .get_mut(opponent_id)
                .captain
                .status_effects
                .push(status(StatusEffectType::Burn, 2, 1));
        }
        Some(Element::Ice) => {
            state
                .players
                .get_mut(opponent_id)
                .captain
                .status_effects
                // 2 turns so it survives the start-of-turn decrement and
                // actually skips the captain's next action.
                .push(status(StatusEffectType::Freeze, 2, 0));
        }
        Some(Element::Poison) => {
            state
                .players
                .get_mut(opponent_id)
                .captain
                .status_effects
                .push(status(StatusEffectType::Poison, -1, 1));
        }
        Some(Element::Thunder) => {
            if let Some(slot) = state.players.get(opponent_id).captain.slot {
                for adj_slot in get_adjacent_slots(slot) {
                    let adj_id = state.players.get(opponent_id).board.get(*adj_slot).cloned();
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
        Some(Element::Sand) => {
            let cap_def_id = state.players.get(opponent_id).captain.def_id.clone();
            let cap_def = registry.get_captain_def(&cap_def_id)?;
            let cap = &mut state.players.get_mut(opponent_id).captain;
            cap.pv_max_loss = Some(cap.pv_max_loss.unwrap_or(0) + 1);
            let printed = if cap.flipped {
                cap_def.verso.pv
            } else {
                cap_def.recto.pv
            };
            let max_pv = printed - cap.pv_max_loss.unwrap_or(0);
            if cap.current_pv > max_pv {
                cap.current_pv = max_pv;
            }
        }
        Some(Element::Water) => {
            let cap = &state.players.get(opponent_id).captain;
            let cap_flipped = cap.flipped;
            let cap_def = registry.get_captain_def(&cap.def_id)?;
            if captain_has_trait(cap_def, cap_flipped, Trait::Cursed) {
                state.players.get_mut(opponent_id).captain.current_pv -= pending.raw_damage;
            }
        }
        None => {}
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
            embargo_turns: None,
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
            survive_target_id: None,
            damage_reduction: None,
            ignore_def: None,
            permanent_pv_loss: None,
            no_heal: None,
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
        let err = declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &attacker, "ghost", false).unwrap_err();
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

        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();

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

        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();

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

        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();

        assert!(state.pending_attack.is_none());
        assert_eq!(
            state.log[0].message,
            "Piege ! Attaquant subit 3 degats en attaquant !"
        );
        assert_eq!(state.log[1].message, "Attaquant est KO par le piege !");
        assert_eq!(state.card(&atk).unwrap().zone, Zone::Graveyard);
        // Decision §8.12: a trap KO is a KO — this test used to assert the
        // removal alone, because the TS path granted neither the +2 Volonte
        // nor the on-KO triggers.
        assert_eq!(state.players.player1.volonte, 12);
        assert_eq!(state.players.player1.ally_ko_ed_this_turn, Some(true));
        assert_eq!(state.players.player1.char_ko_ed_this_game, Some(true));
    }

    /// Decision §8.14 — this test encoded the bug: `captainBonus` used to
    /// *replace* `amount` (2 -> 4). It is a bonus, as its name says, so it now
    /// adds (2 + 4 = 6) and the reduction is recorded on the pending attack
    /// for §8.13.
    #[test]
    fn counter_reduce_adds_the_captain_bonus_against_a_captain() {
        let (mut state, reg) = setup();
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let c = give(&mut state, "X-RED", PlayerId::Player2);
        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, "captain", true).unwrap();
        // ATK 6 vs captain DEF 1 = 5
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 5);

        apply_counter_reduce(&mut state, &reg, &c).unwrap();

        // amount 2 + captainBonus 4 = 6, floored at 0 damage.
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 0);
        assert_eq!(
            state.pending_attack.as_ref().unwrap().damage_reduction,
            Some(6)
        );
        assert_eq!(last_log(&state), "Joue Esquive : reduit les degats de 6");
        assert!(state.players.player2.hand.is_empty());
        assert_eq!(state.card(&c).unwrap().zone, Zone::Graveyard);
    }

    /// The same counter against a character reduces by `amount` alone.
    #[test]
    fn counter_reduce_ignores_the_captain_bonus_against_a_character() {
        let (mut state, reg) = setup();
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-DEF", PlayerId::Player2, Slot::V1);
        let c = give(&mut state, "X-RED", PlayerId::Player2);
        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();
        // ATK 6 vs DEF 5 = 1
        apply_counter_reduce(&mut state, &reg, &c).unwrap();
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 0);
        assert_eq!(last_log(&state), "Joue Esquive : reduit les degats de 2");
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

        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();
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

        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 6);
        apply_counter_survive(&mut state, &reg, &c).unwrap();
        assert!(survive_played(&state));
        assert_eq!(last_log(&state), "Joue Survie : survie a 1 PV !");
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        assert_eq!(state.card(&tgt).unwrap().current_pv, 1);
        assert_eq!(state.card(&tgt).unwrap().zone, Zone::Board);
        assert!(state.pending_attack.is_none());
        // Decision §8.15: the once-per-character tag is spent by the landed
        // attack, not by the counter.
        assert!(state.card(&tgt).unwrap().used_once_abilities.iter().any(|a| a == "survived"));

        // A second attack (no counter) must NOT be saved again: the flag lived
        // on the pending attack, not on the character.
        state.card_mut(&atk).unwrap().tapped = false;
        state.card_mut(&atk).unwrap().used_base_action = false;
        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();
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

        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();
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

        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();
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

        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();
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

        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();
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

        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();
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

        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();
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

        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, "captain", true).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        assert_eq!(state.players.player2.captain.current_pv, 20);
        assert!(state.log.iter().any(|l| l.message
            == "⚠ Logia : INTANGIBILITE LOGIA ! L'attaque passe a travers. Utilisez le Haki (T7+) ou l'Eau pour le toucher."));

        // Turn 7+ grants Armament Haki: the same attack now lands.
        state.turn_number = 7;
        state.card_mut(&atk).unwrap().tapped = false;
        state.card_mut(&atk).unwrap().used_base_action = false;
        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, "captain", true).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();
        assert_eq!(state.players.player2.captain.current_pv, 15);
        assert_eq!(last_log(&state), "Capitaine Logia subit 5 degats (PV: 15)");
    }


    // ============================================================
    // Decision §8.12 — the trap KO is a KO
    // ============================================================

    /// A special refused for cost must not have fired the trap first. The TS
    /// order let a lethal trap KO the attacker and return `Ok`, *then* the
    /// affordability check never ran at all.
    #[test]
    fn an_unaffordable_special_no_longer_eats_the_trap() {
        let (mut state, mut reg) = setup();
        let mut spec_char = character("C-SPEC", "Specialiste", 6, 0, 8);
        spec_char.special_attack = Some(SpecialAttack {
            name: "Boum".to_string(),
            cost: 3,
            atk_bonus: 2,
            ..SpecialAttack::default()
        });
        reg.register_card(spec_char);
        let atk = place(&mut state, &reg, "C-SPEC", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-DEF", PlayerId::Player2, Slot::V1);
        {
            let a = state.card_mut(&atk).unwrap();
            a.current_pv = 2;
            a.status_effects.push(StatusEffect {
                effect_type: StatusEffectType::Trap,
                turns_remaining: -1,
                damage_per_turn: 3,
                source: "t".into(),
            });
        }
        state.players.player1.volonte = 1;
        let ctx = EngineContext::seeded(1);

        let err =
            declare_special_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap_err();
        assert_eq!(err, EngineError::illegal("Cannot afford special (cost 3)"));
        // Alive, untapped, and the trap is still armed.
        assert_eq!(state.card(&atk).unwrap().zone, Zone::Board);
        assert_eq!(state.card(&atk).unwrap().current_pv, 2);
        assert!(state.card(&atk).unwrap().has_status(StatusEffectType::Trap));
        assert!(state.log.is_empty());

        // Affordable now: the trap fires exactly once and the attack goes on.
        state.players.player1.volonte = 5;
        state.card_mut(&atk).unwrap().current_pv = 6;
        declare_special_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        assert_eq!(state.card(&atk).unwrap().current_pv, 3);
        assert!(!state.card(&atk).unwrap().has_status(StatusEffectType::Trap));
        assert!(state.pending_attack.is_some());
    }

    // ============================================================
    // Decision §8.13 — the Bouclier block validates the block
    // ============================================================

    /// A DEF-5 Bouclier ally with no damage-reduction passive.
    fn shield_card(id: &str, def: i32) -> CardDef {
        let mut c = character(id, "Bouclier", 1, def, 9);
        c.traits = Some(vec![Trait::Shield]);
        c
    }

    #[test]
    fn shield_block_refuses_an_illegal_blocker() {
        let (mut state, mut reg) = setup();
        reg.register_card(shield_card("C-SHIELD", 2));
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        // V1's adjacency is [V2, A1]: V3 is not adjacent.
        let far = place(&mut state, &reg, "C-SHIELD", PlayerId::Player2, Slot::V3);
        let mine = place(&mut state, &reg, "C-SHIELD", PlayerId::Player1, Slot::V2);
        let near = place(&mut state, &reg, "C-SHIELD", PlayerId::Player2, Slot::V2);
        let ctx = EngineContext::seeded(1);
        declare_base_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();

        assert_eq!(
            apply_shield_block(&mut state, &reg, "ghost").unwrap_err(),
            EngineError::illegal("Blocker not found")
        );
        assert_eq!(
            apply_shield_block(&mut state, &reg, &mine).unwrap_err(),
            EngineError::illegal("Blocker is not yours")
        );
        assert_eq!(
            apply_shield_block(&mut state, &reg, &tgt).unwrap_err(),
            EngineError::illegal("The target cannot block for itself")
        );
        assert_eq!(
            apply_shield_block(&mut state, &reg, &far).unwrap_err(),
            EngineError::illegal("Blocker is not adjacent to the target")
        );
        // Off the board: graveyarded blockers cannot step in.
        state.card_mut(&near).unwrap().zone = Zone::Graveyard;
        assert_eq!(
            apply_shield_block(&mut state, &reg, &near).unwrap_err(),
            EngineError::illegal("Blocker is not on the board")
        );
        state.card_mut(&near).unwrap().zone = Zone::Board;
        state.card_mut(&near).unwrap().tapped = true;
        assert_eq!(
            apply_shield_block(&mut state, &reg, &near).unwrap_err(),
            EngineError::illegal("A tapped character cannot use Bouclier")
        );
        // Nothing was mutated by the refusals.
        assert_eq!(state.pending_attack.as_ref().unwrap().target_id, tgt);

        state.card_mut(&near).unwrap().tapped = false;
        apply_shield_block(&mut state, &reg, &near).unwrap();
        assert_eq!(state.pending_attack.as_ref().unwrap().target_id, near);
    }

    #[test]
    fn shield_block_keeps_a_reduction_already_paid_for() {
        let (mut state, mut reg) = setup();
        reg.register_card(shield_card("C-SHIELD", 2));
        reg.register_card(counter(
            "X-WALL",
            "Mur d'Acier",
            1,
            CounterEffect::ReduceDamage {
                amount: 5,
                captain_bonus: None,
            },
        ));
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let blk = place(&mut state, &reg, "C-SHIELD", PlayerId::Player2, Slot::V2);
        let wall = give(&mut state, "X-WALL", PlayerId::Player2);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 6);
        apply_counter_reduce(&mut state, &reg, &wall).unwrap();
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 1);

        apply_shield_block(&mut state, &reg, &blk).unwrap();
        // ATK 6 - DEF 2 = 4, minus the -5 already paid for = 0, not 4.
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 0);
    }

    #[test]
    fn shield_block_honours_piercing_and_ignore_def() {
        let (mut state, mut reg) = setup();
        reg.register_card(shield_card("C-SHIELD", 5));
        // A piercing attacker: the keyword is on the *character*, which the
        // TS block never looked at.
        let mut piercer = character("C-PIERCE", "Perceur", 6, 0, 8);
        piercer.traits = Some(vec![Trait::Piercing]);
        reg.register_card(piercer);
        // A special with `ignoreDef: 2`.
        let mut sapper = character("C-SAP", "Sapeur", 6, 0, 8);
        sapper.special_attack = Some(SpecialAttack {
            name: "Sape".to_string(),
            cost: 1,
            atk_bonus: 0,
            ignore_def: Some(2),
            ..SpecialAttack::default()
        });
        reg.register_card(sapper);
        let ctx = EngineContext::seeded(1);

        let mut state_p = state.clone();
        let atk = place(&mut state_p, &reg, "C-PIERCE", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state_p, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let blk = place(&mut state_p, &reg, "C-SHIELD", PlayerId::Player2, Slot::V2);
        declare_base_attack(&mut state_p, &reg, &ctx, &atk, &tgt, false).unwrap();
        apply_shield_block(&mut state_p, &reg, &blk).unwrap();
        // DEF 5 -> floor(5/2) = 2, so 6 - 2 = 4 (the TS block used the full 5).
        assert_eq!(state_p.pending_attack.as_ref().unwrap().raw_damage, 4);

        let atk = place(&mut state, &reg, "C-SAP", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let blk = place(&mut state, &reg, "C-SHIELD", PlayerId::Player2, Slot::V2);
        declare_special_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        apply_shield_block(&mut state, &reg, &blk).unwrap();
        // DEF 5 - ignoreDef 2 = 3, so 6 - 3 = 3.
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 3);
    }

    // ============================================================
    // Decision §8.15 / §8.16 — survive protects the ally it named
    // ============================================================

    #[test]
    fn a_cancelled_attack_does_not_burn_the_survive_tag() {
        let (mut state, reg) = setup();
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let surv = give(&mut state, "X-SURV", PlayerId::Player2);
        let cancel = give(&mut state, "X-CANCEL", PlayerId::Player2);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        // The cancel counter only takes attackers of ATK <= 4.
        state.pending_attack.as_mut().unwrap().attack_power = Some(3);
        apply_counter_survive(&mut state, &reg, &surv).unwrap();
        assert_eq!(
            state.pending_attack.as_ref().unwrap().survive_target_id,
            Some(tgt.clone())
        );
        // The ally is NOT tagged yet.
        assert!(state.card(&tgt).unwrap().used_once_abilities.is_empty());

        apply_counter_cancel(&mut state, &reg, &cancel).unwrap();
        assert!(state.pending_attack.is_none());
        assert!(state.card(&tgt).unwrap().used_once_abilities.is_empty());

        // …so the ally may still be saved later on.
        state.card_mut(&atk).unwrap().tapped = false;
        state.card_mut(&atk).unwrap().used_base_action = false;
        let surv2 = {
            let id = "X-SURV@hand2:player2".to_string();
            let mut inst =
                CardInstance::new(id.clone(), "X-SURV".to_string(), PlayerId::Player2, 0);
            inst.zone = Zone::Hand;
            state.cards.insert(id.clone(), inst);
            state.players.player2.hand.push(id.clone());
            id
        };
        declare_base_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        apply_counter_survive(&mut state, &reg, &surv2).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();
        assert_eq!(state.card(&tgt).unwrap().current_pv, 1);
        assert!(
            state
                .card(&tgt)
                .unwrap()
                .used_once_abilities
                .iter()
                .any(|a| a == "survived")
        );
    }

    #[test]
    fn a_shield_block_after_survive_lets_the_blocker_die() {
        let (mut state, mut reg) = setup();
        let mut blocker = shield_card("C-SHIELD", 0);
        blocker.pv = Some(3);
        reg.register_card(blocker);
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let blk = place(&mut state, &reg, "C-SHIELD", PlayerId::Player2, Slot::V2);
        let surv = give(&mut state, "X-SURV", PlayerId::Player2);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        apply_counter_survive(&mut state, &reg, &surv).unwrap();
        apply_shield_block(&mut state, &reg, &blk).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        // The counter named the ally in V1, not the blocker: 6 damage kill.
        assert_eq!(state.card(&blk).unwrap().zone, Zone::Graveyard);
        assert!(state.card(&blk).unwrap().used_once_abilities.is_empty());
        // The protected ally was never hit, so its tag is still unspent.
        assert_eq!(state.card(&tgt).unwrap().current_pv, 3);
        assert!(state.card(&tgt).unwrap().used_once_abilities.is_empty());
    }

    #[test]
    fn survive_still_refuses_a_captain_target() {
        let (mut state, reg) = setup();
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let surv = give(&mut state, "X-SURV", PlayerId::Player2);
        let ctx = EngineContext::seeded(1);
        declare_base_attack(&mut state, &reg, &ctx, &atk, "captain", true).unwrap();
        assert_eq!(
            apply_counter_survive(&mut state, &reg, &surv).unwrap_err(),
            EngineError::illegal("Survive only protects allies")
        );
        // §8.16: and the captain damage path has no survive branch left.
        resolve_attack(&mut state, &reg, &ctx).unwrap();
        assert_eq!(state.players.player2.captain.current_pv, 15);
    }

    // ============================================================
    // Decision §8.18 — elements resolve off the entry snapshot
    // ============================================================

    #[test]
    fn thunder_still_propagates_off_a_killing_blow() {
        let (mut state, mut reg) = setup();
        let mut thunder = character("C-THUNDER", "Foudre", 6, 0, 8);
        thunder.base_action = Some(BaseAction {
            name: "Eclair".to_string(),
            atk: 6,
            element: Some(Element::Thunder),
            ..Default::default()
        });
        reg.register_card(thunder);
        let atk = place(&mut state, &reg, "C-THUNDER", PlayerId::Player1, Slot::V1);
        // C-ADJ has 3 PV and 0 DEF: the 6-damage blow kills it outright.
        let tgt = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V2);
        let adj = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        assert_eq!(state.card(&tgt).unwrap().zone, Zone::Graveyard);
        // max(1, floor(6/2)) = 3 → the 3-PV neighbour dies too, through the
        // normal KO chain (the TS engine propagated nothing at all here).
        assert_eq!(state.card(&adj).unwrap().zone, Zone::Graveyard);
        assert!(state.log.iter().any(|l| l.message == "Voisin est KO (foudre) !"));
        // +2 Volonte for each lost ally.
        assert_eq!(state.players.player2.volonte, 14);
    }

    #[test]
    fn the_other_elements_stay_no_ops_on_a_dead_target() {
        let (mut state, mut reg) = setup();
        let mut burner = character("C-FIRE", "Feu", 6, 0, 8);
        burner.base_action = Some(BaseAction {
            name: "Flamme".to_string(),
            atk: 6,
            element: Some(Element::Fire),
            ..Default::default()
        });
        reg.register_card(burner);
        let atk = place(&mut state, &reg, "C-FIRE", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        assert_eq!(state.card(&tgt).unwrap().zone, Zone::Graveyard);
        assert!(!state.card(&tgt).unwrap().has_status(StatusEffectType::Burn));
    }

    // ============================================================
    // Decision §8.23 — `Modifier.stat == "haki"`
    // ============================================================

    #[test]
    fn a_haki_modifier_on_the_attacker_pierces_logia() {
        let (mut state, mut reg) = setup();
        let mut logia = character("C-LOGIA", "Logia", 1, 0, 4);
        logia.traits = Some(vec![Trait::Logia]);
        reg.register_card(logia);
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-LOGIA", PlayerId::Player2, Slot::V1);
        let ctx = EngineContext::seeded(1);
        state.card_mut(&atk).unwrap().modifiers.push(Modifier {
            id: "haki_test".to_string(),
            stat: ModifierStat::Haki,
            amount: 1,
            source: "test".to_string(),
            duration: ModifierDuration::Turn,
            turns_remaining: None,
        });

        declare_base_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        assert!(state.pending_attack.as_ref().unwrap().has_haki);
        resolve_attack(&mut state, &reg, &ctx).unwrap();
        assert_eq!(state.card(&tgt).unwrap().zone, Zone::Graveyard);

        // Without the modifier the Logia is intangible again.
        let mut state2 = setup().0;
        let atk = place(&mut state2, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state2, &reg, "C-LOGIA", PlayerId::Player2, Slot::V1);
        declare_base_attack(&mut state2, &reg, &ctx, &atk, &tgt, false).unwrap();
        assert!(!state2.pending_attack.as_ref().unwrap().has_haki);
        resolve_attack(&mut state2, &reg, &ctx).unwrap();
        assert_eq!(state2.card(&tgt).unwrap().current_pv, 4);
    }

    // ============================================================
    // Decision §8.38 — the attack fields combat never honoured
    // ============================================================

    #[test]
    fn impact_alone_pushes_the_target_back_unless_it_is_immune() {
        let (mut state, mut reg) = setup();
        let mut impactor = character("C-IMPACT", "Choc", 6, 0, 8);
        impactor.base_action = Some(BaseAction {
            name: "Poing de l'Amour".to_string(),
            atk: 6,
            attack_traits: Some(vec![AttackTrait::Impact]),
            ..Default::default()
        });
        reg.register_card(impactor);
        let mut rubber = character("C-IMMUNE", "Elastique", 2, 5, 9);
        rubber.passive = Some(passive("Elastique", vec![PassiveEffect::ImmuneImpact]));
        reg.register_card(rubber);
        let ctx = EngineContext::seeded(1);

        let atk = place(&mut state, &reg, "C-IMPACT", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-DEF", PlayerId::Player2, Slot::V1);
        declare_base_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        // The attack sets no `pushback` flag — only the printed "Impact."
        assert_eq!(state.pending_attack.as_ref().unwrap().pushback, None);
        resolve_attack(&mut state, &reg, &ctx).unwrap();
        assert_eq!(state.card(&tgt).unwrap().slot, Some(Slot::A1));
        assert_eq!(state.players.player2.board.get(Slot::A1), Some(&tgt));
        assert_eq!(state.players.player2.board.get(Slot::V1), None);

        // `immuneImpact` (Luffy verso) absorbs it.
        let (mut state2, _) = setup();
        let atk = place(&mut state2, &reg, "C-IMPACT", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state2, &reg, "C-IMMUNE", PlayerId::Player2, Slot::V1);
        declare_base_attack(&mut state2, &reg, &ctx, &atk, &tgt, false).unwrap();
        resolve_attack(&mut state2, &reg, &ctx).unwrap();
        assert_eq!(state2.card(&tgt).unwrap().slot, Some(Slot::V1));
    }

    #[test]
    fn permanent_pv_loss_lowers_the_max_pv_for_good() {
        let (mut state, mut reg) = setup();
        let mut sandman = character("C-SAND", "Sable", 6, 0, 8);
        sandman.special_attack = Some(SpecialAttack {
            name: "Desert Girasol".to_string(),
            cost: 1,
            atk_bonus: 0,
            element: Some(Element::Sand),
            permanent_pv_loss: Some(2),
            ..SpecialAttack::default()
        });
        reg.register_card(sandman);
        let atk = place(&mut state, &reg, "C-SAND", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-DEF", PlayerId::Player2, Slot::V1);
        let ctx = EngineContext::seeded(1);

        declare_special_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        assert_eq!(
            state.pending_attack.as_ref().unwrap().permanent_pv_loss,
            Some(2)
        );
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        // PV 6 - (ATK 6 vs DEF 5 = 1) - 1 (sand element) = 4, and the maximum
        // dropped to 6 - 2 = 4.
        assert_eq!(state.card(&tgt).unwrap().pv_max_loss, Some(2));
        assert_eq!(state.card(&tgt).unwrap().current_pv, 4);
        // A later heal cannot climb past the new maximum.
        crate::board::heal_unit(&mut state, &reg, &tgt, 5).unwrap();
        assert_eq!(state.card(&tgt).unwrap().current_pv, 4);
    }

    #[test]
    fn a_no_heal_special_blocks_later_heals() {
        let (mut state, mut reg) = setup();
        let mut drier = character("C-DRY", "Dessechement", 6, 0, 8);
        drier.special_attack = Some(SpecialAttack {
            name: "Dessiccation".to_string(),
            cost: 1,
            atk_bonus: 0,
            no_heal: Some(true),
            ..SpecialAttack::default()
        });
        reg.register_card(drier);
        let atk = place(&mut state, &reg, "C-DRY", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-DEF", PlayerId::Player2, Slot::V1);
        let ctx = EngineContext::seeded(1);

        declare_special_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();
        assert!(state.card(&tgt).unwrap().has_status(StatusEffectType::NoHeal));
        assert_eq!(state.card(&tgt).unwrap().current_pv, 5);
        assert_eq!(crate::board::heal_unit(&mut state, &reg, &tgt, 3).unwrap(), 0);
    }

    #[test]
    fn a_support_special_buffs_and_cleanses_without_a_pending_attack() {
        let (mut state, mut reg) = setup();
        let mut doc = character("C-DOC", "Hongo", 1, 2, 4);
        doc.special_attack = Some(SpecialAttack {
            name: "Stimulant".to_string(),
            cost: 2,
            atk_bonus: 0,
            is_support: Some(true),
            buff_ally_atk: Some(2),
            cleanse: Some(true),
            ..SpecialAttack::default()
        });
        reg.register_card(doc);
        let caster = place(&mut state, &reg, "C-DOC", PlayerId::Player1, Slot::A1);
        let ally = place(&mut state, &reg, "C-ADJ", PlayerId::Player1, Slot::V1);
        state
            .card_mut(&ally)
            .unwrap()
            .status_effects
            .push(StatusEffect {
                effect_type: StatusEffectType::Freeze,
                turns_remaining: 2,
                damage_per_turn: 0,
                source: "x".into(),
            });
        let ctx = EngineContext::seeded(1);

        declare_special_attack(&mut state, &reg, &ctx, &caster, &ally, false).unwrap();

        // No blow to counter: the support special resolves on the spot.
        assert!(state.pending_attack.is_none());
        assert_eq!(state.players.player1.volonte, 8);
        assert!(state.card(&caster).unwrap().tapped);
        assert_eq!(get_effective_atk(&state, &reg, &ally).unwrap(), 4);
        assert!(!state.card(&ally).unwrap().has_status(StatusEffectType::Freeze));
    }

    #[test]
    fn a_support_special_heals_through_the_single_heal_path() {
        let (mut state, mut reg) = setup();
        let mut medic = character("C-MEDIC", "Medecin", 1, 2, 4);
        medic.special_attack = Some(SpecialAttack {
            name: "Soins Intensifs".to_string(),
            cost: 1,
            atk_bonus: 0,
            is_support: Some(true),
            heal_amount: Some(3),
            ..SpecialAttack::default()
        });
        reg.register_card(medic);
        let caster = place(&mut state, &reg, "C-MEDIC", PlayerId::Player1, Slot::A1);
        let ally = place(&mut state, &reg, "C-DEF", PlayerId::Player1, Slot::V1);
        state.card_mut(&ally).unwrap().current_pv = 2;
        let ctx = EngineContext::seeded(1);

        declare_special_attack(&mut state, &reg, &ctx, &caster, &ally, false).unwrap();
        assert!(state.pending_attack.is_none());
        assert_eq!(state.card(&ally).unwrap().current_pv, 5);

        // Decision §8.5: the heal never climbs past the printed PV…
        state.card_mut(&caster).unwrap().used_special_attack = false;
        state.card_mut(&caster).unwrap().tapped = false;
        declare_special_attack(&mut state, &reg, &ctx, &caster, &ally, false).unwrap();
        assert_eq!(state.card(&ally).unwrap().current_pv, 6);

        // …and a `noHeal` unit is skipped entirely.
        state.card_mut(&caster).unwrap().used_special_attack = false;
        state.card_mut(&caster).unwrap().tapped = false;
        state.card_mut(&ally).unwrap().current_pv = 1;
        state
            .card_mut(&ally)
            .unwrap()
            .status_effects
            .push(StatusEffect {
                effect_type: StatusEffectType::NoHeal,
                turns_remaining: 2,
                damage_per_turn: 0,
                source: "x".into(),
            });
        declare_special_attack(&mut state, &reg, &ctx, &caster, &ally, false).unwrap();
        assert_eq!(state.card(&ally).unwrap().current_pv, 1);
    }

    #[test]
    fn pushback_slots_is_one_pushback() {
        let (mut state, mut reg) = setup();
        let mut shover = character("C-SHOVE", "Bourrade", 6, 0, 8);
        shover.special_attack = Some(SpecialAttack {
            name: "Bourrade".to_string(),
            cost: 1,
            atk_bonus: 0,
            // The board has exactly two rows, so "N slots" is one row back.
            pushback_slots: Some(2),
            ..SpecialAttack::default()
        });
        reg.register_card(shover);
        let atk = place(&mut state, &reg, "C-SHOVE", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-DEF", PlayerId::Player2, Slot::V1);
        let ctx = EngineContext::seeded(1);
        declare_special_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        assert_eq!(state.pending_attack.as_ref().unwrap().pushback, Some(true));
        resolve_attack(&mut state, &reg, &ctx).unwrap();
        assert_eq!(state.card(&tgt).unwrap().slot, Some(Slot::A1));
    }

    #[test]
    fn a_taunt_binds_the_enemy_to_its_taunter() {
        let (mut state, mut reg) = setup();
        let mut rockstar = character("C-TAUNT", "Rockstar", 3, 2, 4);
        rockstar.special_attack = Some(SpecialAttack {
            name: "Provocation".to_string(),
            cost: 1,
            atk_bonus: 0,
            is_support: Some(true),
            taunt: Some(true),
            ..SpecialAttack::default()
        });
        reg.register_card(rockstar);
        let taunter = place(&mut state, &reg, "C-TAUNT", PlayerId::Player1, Slot::V1);
        let other = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V2);
        let enemy = place(&mut state, &reg, "C-ATK", PlayerId::Player2, Slot::V1);
        let ctx = EngineContext::seeded(1);

        let before = crate::board::get_valid_targets(&state, &reg, &enemy, false).unwrap();
        assert_eq!(before.character_targets, vec![taunter.clone(), other.clone()]);

        declare_special_attack(&mut state, &reg, &ctx, &taunter, &enemy, false).unwrap();
        assert!(state.pending_attack.is_none());
        assert!(state.card(&enemy).unwrap().has_status(StatusEffectType::Taunt));

        let bound = crate::board::get_valid_targets(&state, &reg, &enemy, false).unwrap();
        assert_eq!(bound.character_targets, vec![taunter.clone()]);
        assert!(!bound.can_target_captain);

        // Once the taunt lapses the enemy is free again.
        state.card_mut(&enemy).unwrap().status_effects.clear();
        let free = crate::board::get_valid_targets(&state, &reg, &enemy, false).unwrap();
        assert_eq!(free.character_targets, vec![taunter, other]);
    }

    #[test]
    fn base_action_cannot_be_dodged_is_read_into_the_pending_attack() {
        let (mut state, mut reg) = setup();
        let mut sniper = character("C-NODODGE", "Kizaru", 6, 0, 8);
        sniper.base_action = Some(BaseAction {
            name: "Lumiere".to_string(),
            atk: 6,
            cannot_be_dodged: Some(true),
            ..Default::default()
        });
        reg.register_card(sniper);
        let atk = place(&mut state, &reg, "C-NODODGE", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        declare_base_attack(&mut state, &reg, &EngineContext::seeded(1), &atk, &tgt, false).unwrap();
        assert_eq!(
            state.pending_attack.as_ref().unwrap().cannot_be_dodged,
            Some(true)
        );
    }

    // ============================================================
    // Decision §8.39 — the whole Transform
    // ============================================================

    #[test]
    fn monster_point_applies_atk_def_pv_and_rush() {
        let (mut state, mut reg) = setup();
        let mut chopper = character("C-CHOP", "Chopper", 2, 1, 5);
        chopper.special_attack = Some(SpecialAttack {
            name: "Monster Point".to_string(),
            cost: 3,
            atk_bonus: 0,
            once_per_game: Some(true),
            is_support: Some(true),
            transform: Some(crate::types::Transform {
                atk: 8,
                def: 2,
                pv: 6,
                turns: 2,
            }),
            ..SpecialAttack::default()
        });
        // A transform that *lowers* the stats, to prove the deltas are signed.
        let mut mini = character("C-MINI", "Mini", 9, 4, 9);
        mini.special_attack = Some(SpecialAttack {
            name: "Mini Point".to_string(),
            cost: 1,
            atk_bonus: 0,
            transform: Some(crate::types::Transform {
                atk: 3,
                def: 1,
                pv: 4,
                turns: 1,
            }),
            ..SpecialAttack::default()
        });
        reg.register_card(chopper);
        reg.register_card(mini);
        let ctx = EngineContext::seeded(1);

        let chop = place(&mut state, &reg, "C-CHOP", PlayerId::Player1, Slot::V1);
        declare_special_attack(&mut state, &reg, &ctx, &chop, &chop, false).unwrap();

        assert!(state.pending_attack.is_none());
        assert_eq!(get_effective_atk(&state, &reg, &chop).unwrap(), 8);
        assert_eq!(get_effective_def(&state, &reg, &chop).unwrap(), 2);
        assert_eq!(state.card(&chop).unwrap().current_pv, 6);
        // Rush for the duration: the `-1` sentinel `grantSelfRush` writes, so
        // the monster form is never summoning-sick.
        assert_eq!(state.card(&chop).unwrap().deployed_turn, Some(-1));
        state.turn_number += 1;
        assert!(!has_summoning_sickness(&state, &reg, &chop).unwrap());
        assert_eq!(
            state
                .card(&chop)
                .unwrap()
                .status(StatusEffectType::SelfKo)
                .unwrap()
                .turns_remaining,
            2
        );
        assert_eq!(
            last_log(&state),
            "Chopper : Monster Point ! ATK 8 pendant 2 tours, puis KO."
        );

        let mini = place(&mut state, &reg, "C-MINI", PlayerId::Player1, Slot::V2);
        declare_special_attack(&mut state, &reg, &ctx, &mini, &mini, false).unwrap();
        assert_eq!(get_effective_atk(&state, &reg, &mini).unwrap(), 3);
        assert_eq!(get_effective_def(&state, &reg, &mini).unwrap(), 1);
        assert_eq!(state.card(&mini).unwrap().current_pv, 4);
    }

    // ============================================================
    // Decision §8.40 — captains feel every element, Logia once a turn
    // ============================================================

    /// Declare a base attack with `element` at the enemy captain and resolve it.
    fn hit_captain_with(
        element: Element,
        cursed_verso: bool,
        captain_slot: Option<Slot>,
    ) -> (GameState, CardRegistry) {
        let (mut state, mut reg) = setup();
        let mut cap = captain_def("CAP-E", "Element", 1, false);
        if cursed_verso {
            cap.verso.traits = Some(vec![Trait::Cursed]);
        }
        reg.register_captain(cap);
        let mut elem = character("C-ELEM", "Elementaire", 6, 0, 8);
        elem.base_action = Some(BaseAction {
            name: "Coup".to_string(),
            atk: 6,
            element: Some(element),
            ..Default::default()
        });
        reg.register_card(elem);

        state.players.player2.captain =
            CaptainInstance::new("CAP-E".into(), PlayerId::Player2, 20);
        if let Some(slot) = captain_slot {
            state.players.player2.captain.flipped = true;
            state.players.player2.captain.slot = Some(slot);
        }
        let atk = place(&mut state, &reg, "C-ELEM", PlayerId::Player1, Slot::V1);
        let ctx = EngineContext::seeded(1);
        declare_base_attack(&mut state, &reg, &ctx, &atk, "captain", true).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();
        (state, reg)
    }

    #[test]
    fn every_element_now_lands_on_a_captain() {
        // fire → burn 2t / 1 dmg
        let (state, _) = hit_captain_with(Element::Fire, false, None);
        let cap = &state.players.player2.captain;
        assert_eq!(cap.current_pv, 15);
        let burn = cap
            .status_effects
            .iter()
            .find(|e| e.effect_type == StatusEffectType::Burn)
            .unwrap();
        assert_eq!((burn.turns_remaining, burn.damage_per_turn), (2, 1));

        // ice → freeze 2t
        let (state, _) = hit_captain_with(Element::Ice, false, None);
        assert!(
            state
                .players
                .player2
                .captain
                .has_status(StatusEffectType::Freeze)
        );

        // poison → permanent, and the start-of-turn tick floors it at 1 PV
        let (mut state, _) = hit_captain_with(Element::Poison, false, None);
        let poison = state
            .players
            .player2
            .captain
            .status_effects
            .iter()
            .find(|e| e.effect_type == StatusEffectType::Poison)
            .unwrap();
        assert_eq!((poison.turns_remaining, poison.damage_per_turn), (-1, 1));
        state.players.player2.captain.current_pv = 1;
        state.current_player = PlayerId::Player2;
        state.process_start_of_turn_effects();
        assert_eq!(state.players.player2.captain.current_pv, 1);

        // sand → one point of permanent maximum PV
        let (state, _) = hit_captain_with(Element::Sand, false, None);
        assert_eq!(state.players.player2.captain.pv_max_loss, Some(1));

        // water → the raw damage a second time against a Cursed captain
        let (state, _) = hit_captain_with(Element::Water, true, Some(Slot::V2));
        assert_eq!(state.players.player2.captain.current_pv, 10);
        // …and only then: a captain that is not Cursed takes it once.
        let (state, _) = hit_captain_with(Element::Water, false, Some(Slot::V2));
        assert_eq!(state.players.player2.captain.current_pv, 15);
    }

    #[test]
    fn thunder_on_a_captain_splashes_the_slot_next_to_it() {
        let (mut state, mut reg) = setup();
        let mut elem = character("C-ELEM", "Elementaire", 6, 0, 8);
        elem.base_action = Some(BaseAction {
            name: "Coup".to_string(),
            atk: 6,
            element: Some(Element::Thunder),
            ..Default::default()
        });
        reg.register_card(elem);
        state.players.player2.captain.flipped = true;
        state.players.player2.captain.slot = Some(Slot::V2);
        let atk = place(&mut state, &reg, "C-ELEM", PlayerId::Player1, Slot::V1);
        let neighbour = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &ctx, &atk, "captain", true).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();

        assert_eq!(state.players.player2.captain.current_pv, 15);
        // max(1, floor(5/2)) = 2 → 3 PV - 2 = 1.
        assert_eq!(state.card(&neighbour).unwrap().current_pv, 1);
    }

    #[test]
    fn a_logia_captain_only_ignores_the_first_hit_of_the_turn() {
        let (mut state, reg) = setup();
        state.players.player2.captain = CaptainInstance::new("CAP-L".into(), PlayerId::Player2, 20);
        state.players.player2.captain.flipped = true;
        let a1 = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        let a2 = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V2);
        let ctx = EngineContext::seeded(1);

        declare_base_attack(&mut state, &reg, &ctx, &a1, "captain", true).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();
        assert_eq!(state.players.player2.captain.current_pv, 20);
        assert_eq!(
            state.players.player2.captain.logia_used_this_turn,
            Some(true)
        );

        // Second unhakied hit of the same turn: the intangibility is spent.
        declare_base_attack(&mut state, &reg, &ctx, &a2, "captain", true).unwrap();
        resolve_attack(&mut state, &reg, &ctx).unwrap();
        assert_eq!(state.players.player2.captain.current_pv, 15);

        // The flag is cleared at the start of the captain's own turn.
        state.current_player = PlayerId::Player2;
        state.reset_turn_flags();
        assert_eq!(
            state.players.player2.captain.logia_used_this_turn,
            Some(false)
        );
    }

    // ============================================================
    // Decision §8.55 — DEF is clamped at 0 before the halving
    // ============================================================

    #[test]
    fn a_captain_debuffed_below_zero_def_takes_exactly_the_attack() {
        let (mut state, reg) = setup();
        let atk = place(&mut state, &reg, "C-ATK", PlayerId::Player1, Slot::V1);
        state
            .players
            .player2
            .captain
            .modifiers
            .push(Modifier {
                id: "debuff".to_string(),
                stat: ModifierStat::Def,
                amount: -5,
                source: "test".to_string(),
                duration: ModifierDuration::Turn,
                turns_remaining: None,
            });
        let ctx = EngineContext::seeded(1);
        declare_base_attack(&mut state, &reg, &ctx, &atk, "captain", true).unwrap();
        // Face DEF 1 - 5 = -4, clamped at 0: exactly ATK 6, not 6 + 4.
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 6);
        assert!(last_log(&state).contains("(ATK 6 vs DEF 0 = 6 degats)"));
    }

    #[test]
    fn piercing_a_negative_def_unit_deals_the_plain_attack() {
        let (mut state, mut reg) = setup();
        let mut piercer = character("C-PIERCE", "Perceur", 6, 0, 8);
        piercer.traits = Some(vec![Trait::Piercing]);
        reg.register_card(piercer);
        let atk = place(&mut state, &reg, "C-PIERCE", PlayerId::Player1, Slot::V1);
        let tgt = place(&mut state, &reg, "C-ADJ", PlayerId::Player2, Slot::V1);
        state.card_mut(&tgt).unwrap().modifiers.push(Modifier {
            id: "debuff".to_string(),
            stat: ModifierStat::Def,
            amount: -3,
            source: "test".to_string(),
            duration: ModifierDuration::Turn,
            turns_remaining: None,
        });
        let ctx = EngineContext::seeded(1);
        declare_base_attack(&mut state, &reg, &ctx, &atk, &tgt, false).unwrap();
        // floor(-3 / 2) would be -2 and *raise* the damage to 8.
        assert_eq!(state.pending_attack.as_ref().unwrap().raw_damage, 6);
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

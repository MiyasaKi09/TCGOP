//! The opponent AI — port of `src/engine/ai.ts`.
//!
//! Three difficulties over one action list:
//! - **beginner** — 80 % chance to just take the hit on defence, then a 35 %
//!   uniform-random pick out of a "basic actions" pool, otherwise the score
//!   heuristic with `Math.random() * 7` jitter;
//! - **intermediate** — the pure score heuristic, no jitter;
//! - **expert** — 1-ply lookahead: apply each action to a clone, run
//!   [`evaluate_state`], add `scoreAction * 0.02` as a tie-break and subtract
//!   `6` from `endTurn`; an action that throws scores `-Infinity`.
//!
//! Every `Math.random()` here goes through [`EngineContext::rng`]. The expert
//! search applies each candidate to a **clone of the state** — TS
//! `executeAction` is pure, so the caller's state is never touched — but it
//! runs against the caller's own [`EngineContext`]: `instanceCounter` and the
//! `Math.random()` stream are TS *module globals* that a speculative rollout
//! advances permanently (nothing rolls them back on the way out of the
//! `try`/`catch`), so sharing the context is what reproduces the instance ids
//! and the random stream position the TS engine ends up with.
//!
//! Fallibility note: the TS scoring helpers are fallible, but only through
//! `getCardDef`: `getEffectiveAtk` / `getEffectiveDef` return `0` for an
//! instance that is *missing* from `state.cards` (board.ts:97, board.ts:124),
//! and throw only when a *present* instance — or one of its attached objects —
//! carries a `defId` the registry does not know. `scoreAction` is called
//! from `chooseExpert` *outside* its `try`/`catch` (ai.ts:70), so that throw
//! escapes the whole `aiChooseAction` — hence [`score_action`] and the choosers
//! return [`EngineResult`]. So does `getValidActions` (ai.ts:25, outside any
//! `try`), so [`ai_choose_action`] propagates it rather than acting on a
//! truncated action list. `evaluateState`, by contrast, is called *inside* the
//! `try` of `chooseExpert`, so a failure there is the `-Infinity` branch — but
//! it is still fallible, because an *external* caller of the exported
//! `evaluateState` sees the throw.
//!
//! Decision §8.20 narrows that: `score_equip_object` no longer propagates —
//! an object instance the state does not know, or one whose definition is not
//! registered, scores `0.0` so a single dangling candidate cannot abort the
//! whole AI turn from outside `chooseExpert`'s try/catch.

#![allow(clippy::collapsible_if)]
// ^ The nested `if` / `if let` blocks in this module mirror the TypeScript
// source branch for branch (see the per-function `PORT:` references). Merging
// them into let-chains would break that 1:1 reading, which is the whole point
// of the port, so the lint is turned off for this file only.

use serde::{Deserialize, Serialize};

use crate::actions::get_valid_actions;
use crate::board::{get_board_characters, get_effective_atk, get_effective_def, is_front_slot};
use crate::combat::CAPTAIN_ATTACKER_PREFIX;
use crate::context::EngineContext;
use crate::error::EngineResult;
use crate::execute::execute_action;
use crate::registry::CardRegistry;
use crate::state::GameState;
use crate::types::{EventEffect, GameAction, HakiType, ModifierStat, PlayerId, Row};

/// TS `Difficulty = "beginner" | "intermediate" | "expert"` — `src/engine/ai.ts:12`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Difficulty {
    #[serde(rename = "beginner")]
    Beginner,
    /// TS default parameter of `aiChooseAction`.
    #[default]
    #[serde(rename = "intermediate")]
    Intermediate,
    #[serde(rename = "expert")]
    Expert,
}

/// TS `BASIC` — `src/engine/ai.ts:43`. The action-type discriminators the
/// beginner AI prefers, in set-literal order.
pub const BASIC_ACTION_TYPES: [&str; 7] = [
    "deployCharacter",
    "baseAttack",
    "moveCharacter",
    "endTurn",
    "passCounter",
    "playCounter",
    "useShield",
];

/// TS `aiChooseAction(state, playerId, difficulty = "intermediate")`
/// — `src/engine/ai.ts:20`.
///
/// Falls back to `endTurn` on an empty action list and returns the sole action
/// when there is exactly one, before any difficulty branching.
pub fn ai_choose_action(
    state: &GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    difficulty: Difficulty,
) -> EngineResult<GameAction> {
    let actions = get_valid_actions(state, registry, player_id)?;
    if actions.is_empty() {
        return Ok(GameAction::EndTurn);
    }
    if actions.len() == 1 {
        return Ok(actions[0].clone());
    }
    match difficulty {
        Difficulty::Beginner => choose_beginner(state, registry, ctx, player_id, &actions),
        Difficulty::Expert => choose_expert(state, registry, ctx, player_id, &actions),
        Difficulty::Intermediate => choose_by_score(state, registry, ctx, player_id, &actions, 0.0),
    }
}

/// TS `chooseByScore(state, playerId, actions, jitter)` — `src/engine/ai.ts:33`.
///
/// First action wins ties (`score > bestScore`, strict). `jitter > 0` adds
/// `Math.random() * jitter`.
///
/// An empty list has no TS counterpart (`actions[0]` would be `undefined`);
/// this port returns `endTurn`, matching [`ai_choose_action`]'s own fallback.
pub fn choose_by_score(
    state: &GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    actions: &[GameAction],
    jitter: f64,
) -> EngineResult<GameAction> {
    let Some(first) = actions.first() else {
        return Ok(GameAction::EndTurn);
    };
    let mut best = first;
    let mut best_score = f64::NEG_INFINITY;
    for action in actions {
        let mut score = score_action(state, registry, player_id, action)?;
        if jitter > 0.0 {
            score += ctx.rng.random_f64() * jitter;
        }
        if score > best_score {
            best_score = score;
            best = action;
        }
    }
    Ok(best.clone())
}

/// TS `chooseBeginner(state, playerId, actions)` — `src/engine/ai.ts:45`.
///
/// Defence first: when an attack is pending and a `passCounter` is on offer,
/// one `Math.random()` draw takes the hit 80 % of the time (the draw only
/// happens when such an action exists — `pass && Math.random() < 0.8`). Then
/// the pool is the `BASIC` subset when non-empty, otherwise every action; a
/// second draw plays uniformly at random 35 % of the time (consuming a third
/// draw for the index), else it falls through to [`choose_by_score`] with
/// jitter `7`.
pub fn choose_beginner(
    state: &GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    actions: &[GameAction],
) -> EngineResult<GameAction> {
    if actions.is_empty() {
        return Ok(GameAction::EndTurn);
    }

    // Defense: usually just take the hit.
    if state.pending_attack.is_some() {
        if let Some(pass) = actions
            .iter()
            .find(|a| matches!(a, GameAction::PassCounter))
        {
            if ctx.rng.random_f64() < 0.8 {
                return Ok(pass.clone());
            }
        }
    }

    let simple: Vec<GameAction> = actions
        .iter()
        .filter(|a| BASIC_ACTION_TYPES.contains(&a.type_name()))
        .cloned()
        .collect();
    let pool: &[GameAction] = if !simple.is_empty() { &simple } else { actions };

    // Often plays almost randomly; otherwise a very noisy heuristic.
    if ctx.rng.random_f64() < 0.35 {
        let idx = ctx.rng.random_index(pool.len());
        return Ok(pool[idx].clone());
    }
    choose_by_score(state, registry, ctx, player_id, pool, 7.0)
}

/// TS `chooseExpert(state, playerId, actions)` — `src/engine/ai.ts:58`.
///
/// 1-ply lookahead. Each candidate is applied to a **clone of the state** (TS
/// `executeAction` is pure, so the caller's state never changes) but against
/// the caller's own [`EngineContext`]: `instanceCounter` and the `Math.random()`
/// stream are TS module globals, and a speculative rollout that spawns a token
/// or draws a random number advances them for good — the `catch` does not undo
/// them. An action that errors scores `-Infinity` (the TS `catch`). The
/// heuristic tie-break `scoreAction × 0.02` and the `endTurn` penalty of `6`
/// are added *after* the try/catch, so they apply to failing actions too —
/// `-Infinity` simply absorbs them — and `scoreAction` sits outside the
/// `catch`, so its failure escapes rather than scoring `-Infinity`.
pub fn choose_expert(
    state: &GameState,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
    player_id: PlayerId,
    actions: &[GameAction],
) -> EngineResult<GameAction> {
    let Some(first) = actions.first() else {
        return Ok(GameAction::EndTurn);
    };
    let mut best = first;
    let mut best_val = f64::NEG_INFINITY;
    for action in actions {
        let mut next_state = state.clone();
        // TS `try { const ns = executeAction(state, action); val = evaluateState(ns, playerId); }
        // catch { val = -Infinity; }` — both calls sit inside the same `try`.
        let mut val = execute_action(&mut next_state, registry, ctx, action)
            .and_then(|()| evaluate_state(&next_state, registry, player_id))
            .unwrap_or(f64::NEG_INFINITY);
        // small heuristic tie-break; rank ending the turn last.
        val += score_action(state, registry, player_id, action)? * 0.02;
        if matches!(action, GameAction::EndTurn) {
            val -= 6.0;
        }
        if val > best_val {
            best_val = val;
            best = action;
        }
    }
    Ok(best.clone())
}

/// TS `evaluateState(state, playerId)` — `src/engine/ai.ts:78`.
///
/// `1.5 ×` the captain-PV difference, `±1000` for a dead captain or a decided
/// winner, `atk + def + 0.5 × pv + 3` per own board character (minus the same
/// for the opponent's), `1.4 ×` own hand minus `1.0 ×` enemy hand, `0.4 ×` own
/// Volonté. All arithmetic is `f64` — keep it so the tie-breaks match.
///
/// Fallible like the TS: `getEffectiveAtk` / `getEffectiveDef` return `0` for a
/// *missing instance*, but `throw` from `getCardDef` when a present instance
/// (or one of its attached objects) has an unregistered `defId`. Inside
/// [`choose_expert`] that failure is the `-Infinity` branch, so the candidate
/// action is discarded rather than scored with the missing terms as zeroes.
pub fn evaluate_state(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> EngineResult<f64> {
    let opp = player_id.opponent();
    let me = state.players.get(player_id);
    let op = state.players.get(opp);
    let mut v = 0.0_f64;

    v += (me.captain.current_pv - op.captain.current_pv) as f64 * 1.5;
    if op.captain.current_pv <= 0 {
        v += 1000.0;
    }
    if me.captain.current_pv <= 0 {
        v -= 1000.0;
    }
    if state.winner == Some(player_id) {
        v += 1000.0;
    }
    if state.winner == Some(opp) {
        v -= 1000.0;
    }

    for c in get_board_characters(state, player_id) {
        v += get_effective_atk(state, registry, &c.instance_id)? as f64
            + get_effective_def(state, registry, &c.instance_id)? as f64
            + c.current_pv as f64 * 0.5
            + 3.0;
    }
    for c in get_board_characters(state, opp) {
        v -= get_effective_atk(state, registry, &c.instance_id)? as f64
            + get_effective_def(state, registry, &c.instance_id)? as f64
            + c.current_pv as f64 * 0.5
            + 3.0;
    }

    v += me.hand.len() as f64 * 1.4 - op.hand.len() as f64 * 1.0;
    v += me.volonte as f64 * 0.4;
    Ok(v)
}

/// TS `scoreAction(state, playerId, action)` — `src/engine/ai.ts:102`.
///
/// `deployShip` 15, `playCounter` 50, `passCounter` 0, `useHaki` 40 for
/// observation / 60 for king / 10 otherwise, `moveCharacter` 2, `endTurn` −1,
/// anything not listed 0.
pub fn score_action(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    action: &GameAction,
) -> EngineResult<f64> {
    Ok(match action {
        GameAction::DeployCharacter { .. } => {
            score_deploy_character(state, registry, player_id, action)?
        }
        // Ships are usually good
        GameAction::DeployShip { .. } => 15.0,
        GameAction::EquipObject { .. } => score_equip_object(state, registry, action)?,
        GameAction::BaseAttack { .. } | GameAction::SpecialAttack { .. } => {
            score_attack(state, registry, player_id, action)?
        }
        GameAction::CaptainAttack { .. } | GameAction::UseSurcharge { .. } => {
            score_attack(state, registry, player_id, action)?
        }
        // Decision §8.19: an awakened-fruit swing is one of the strongest
        // attacks in the game; the TS switch omitted it, so it scored 0 —
        // below `moveCharacter`. It is scored like any other special attack.
        GameAction::FruitSpecialAttack { .. } => score_attack(state, registry, player_id, action)?,
        GameAction::PlayEvent { .. } => score_event(state, registry, player_id, action)?,
        // Counters during enemy turn are almost always good
        GameAction::PlayCounter { .. } => 50.0,
        GameAction::UseShield { .. } => score_shield(state, registry, action)?,
        // Default — pass if no counter
        GameAction::PassCounter => 0.0,
        GameAction::FlipCaptain { .. } => score_captain_flip(state, registry, player_id),
        GameAction::UseHaki { haki_type, .. } => match haki_type {
            HakiType::Observation => 40.0,
            HakiType::King => 60.0,
            HakiType::Armament => 10.0,
        },
        // Low priority
        GameAction::MoveCharacter { .. } => 2.0,
        // Last resort
        GameAction::EndTurn => -1.0,
        _ => 0.0,
    })
}

/// TS `scoreDeployCharacter(state, playerId, action)` — `src/engine/ai.ts:140`.
///
/// `10 + 2×atk + pv`, `+10` under 3 board characters, `+20` at zero, `+3` when
/// the slot matches `preferredRow`.
pub fn score_deploy_character(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    action: &GameAction,
) -> EngineResult<f64> {
    let GameAction::DeployCharacter { instance_id, slot } = action else {
        return Ok(0.0);
    };
    // TS `if (!card) return 0;` …
    let Some(card) = state.cards.get(instance_id) else {
        return Ok(0.0);
    };
    // … then `getCardDef(card.defId)`, which throws on an unknown definition.
    let def = registry.get_card_def(&card.def_id)?;

    // Base score for deploying
    let mut score = 10.0_f64;

    // Higher cost cards are generally stronger
    score += def.atk.unwrap_or(0) as f64 * 2.0;
    score += def.pv.unwrap_or(0) as f64;

    // Prefer deploying if we have few board characters
    let board_count = get_board_characters(state, player_id).len();
    if board_count < 3 {
        score += 10.0;
    }
    if board_count == 0 {
        score += 20.0;
    }

    // Prefer support in back, fighters in front
    let is_front = is_front_slot(*slot);
    if def.preferred_row == Some(Row::Front) && is_front {
        score += 3.0;
    }
    if def.preferred_row == Some(Row::Back) && !is_front {
        score += 3.0;
    }

    Ok(score)
}

/// TS `scoreEquipObject(state, action)` — `src/engine/ai.ts:168`:
/// `8 + 3 × bonusAtk`.
///
/// The TS reads `state.cards[objectInstanceId].defId` unguarded, so a missing
/// instance throws a TypeError and an unregistered def id throws from
/// `getCardDef`.
///
/// Decision §8.20: both are `0.0` here instead. Scoring is a heuristic, never a
/// legality check, and [`choose_expert`] calls [`score_action`] *outside* its
/// try/catch — a dangling instance behind one candidate must score badly, not
/// abort the whole AI turn. Same shape as [`score_deploy_character`], which
/// already returns 0 for a missing instance.
pub fn score_equip_object(
    state: &GameState,
    registry: &CardRegistry,
    action: &GameAction,
) -> EngineResult<f64> {
    let GameAction::EquipObject {
        object_instance_id, ..
    } = action
    else {
        return Ok(0.0);
    };
    let Some(obj_card) = state.cards.get(object_instance_id) else {
        return Ok(0.0);
    };
    let Ok(obj_def) = registry.get_card_def(&obj_card.def_id) else {
        return Ok(0.0);
    };
    let mut score = 8.0_f64;
    score += obj_def.bonus_atk.unwrap_or(0) as f64 * 3.0;
    Ok(score)
}

/// TS `scoreAttack(state, playerId, action)` — `src/engine/ai.ts:178`.
///
/// `5`, `+15` against a captain, and against a character `+20` for a lethal hit
/// plus `max(0, 5 - target.currentPv)`; `+5` more for a `specialAttack`.
///
/// TS quirk (fixed here, decision §8.19): the KO block was guarded by
/// `"attackerInstanceId" in action && "targetInstanceId" in action`, so a
/// `captainAttack` — which carries no `attackerInstanceId` — skipped it
/// entirely and scored 5 / 20 flat, below `moveCharacter`'s 2 once the lethal
/// bonus is what should have decided it. The attacker's ATK for a
/// `captainAttack` / `useSurcharge` is now resolved from the captain's **active
/// face** (verso when flipped, recto otherwise) plus the captain's `Atk`
/// modifiers, so the KO block runs for captain swings too. This is a heuristic:
/// it deliberately ignores per-attack `atkBonus` / `conditionalBonus`, which
/// only make the estimate conservative.
pub fn score_attack(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    action: &GameAction,
) -> EngineResult<f64> {
    let mut score = 5.0_f64;
    // TS declares `const opponentId = getOpponent(playerId)` and never reads it.
    let _opponent_id = player_id.opponent();

    let (attacker_id, target_id, target_is_captain) = attack_operands(action);

    // Captain attacks are high value
    if target_is_captain {
        score += 15.0;
    }

    // Decision §8.19: a captain attack has no `attackerInstanceId`, so the
    // attacker side of the KO block comes from the captain instead.
    let is_captain_attack = matches!(
        action,
        GameAction::CaptainAttack { .. } | GameAction::UseSurcharge { .. }
    );

    // Check if we can KO the target
    if let Some(target_id) = target_id {
        if attacker_id.is_some() || is_captain_attack {
            if !target_is_captain {
                if let Some(target) = state.cards.get(target_id) {
                    let target_current_pv = target.current_pv;
                    let attacker_atk = match attacker_id {
                        // Decision §8.28 (follow-up) × §8.19: a fruit special
                        // declared by the *captain* carries the synthetic
                        // `captain_{playerId}` attacker id, which is never a
                        // key of `state.cards` — its ATK comes from the active
                        // face, like every other captain swing, so the AI can
                        // still see lethal with it.
                        Some(id) if !id.starts_with(CAPTAIN_ATTACKER_PREFIX) => {
                            get_effective_atk(state, registry, id)?
                        }
                        _ => captain_effective_atk(state, registry, player_id)?,
                    };
                    let target_def_val = get_effective_def(state, registry, target_id)?;
                    let damage = (attacker_atk - target_def_val).max(0);
                    if damage >= target_current_pv {
                        // Can KO!
                        score += 20.0;
                    }
                    // Prefer targets with low PV
                    score += (5 - target_current_pv).max(0) as f64;
                }
            }
        }
    }

    // Special attacks are big commitments — slightly lower base score
    // (§8.19: `fruitSpecialAttack` is a special attack too).
    if matches!(
        action,
        GameAction::SpecialAttack { .. } | GameAction::FruitSpecialAttack { .. }
    ) {
        // But they do more damage
        score += 5.0;
    }

    Ok(score)
}

/// The attacker-side ATK [`score_attack`] uses for a `captainAttack` /
/// `useSurcharge` (decision §8.19): the **active** face's printed ATK plus the
/// captain's `Atk` modifiers, mirroring what `declare_captain_special_attack`
/// computes before the attack's own bonuses.
fn captain_effective_atk(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> EngineResult<i32> {
    let captain = &state.players.get(player_id).captain;
    let def = registry.get_captain_def(&captain.def_id)?;
    let mut atk = if captain.flipped {
        def.verso.atk
    } else {
        def.recto.atk
    };
    for m in &captain.modifiers {
        if m.stat == ModifierStat::Atk {
            atk += m.amount;
        }
    }
    Ok(atk)
}

/// The JS `"attackerInstanceId" in action` / `"targetInstanceId" in action` /
/// `"targetIsCaptain" in action && action.targetIsCaptain` probes, resolved on
/// the Rust enum. Variants with no attacker id skip the KO block regardless of
/// what else they carry, so only the attack variants need spelling out.
fn attack_operands(action: &GameAction) -> (Option<&str>, Option<&str>, bool) {
    match action {
        GameAction::BaseAttack {
            attacker_instance_id,
            target_instance_id,
            target_is_captain,
        }
        | GameAction::SpecialAttack {
            attacker_instance_id,
            target_instance_id,
            target_is_captain,
        } => (
            Some(attacker_instance_id.as_str()),
            Some(target_instance_id.as_str()),
            *target_is_captain == Some(true),
        ),
        GameAction::FruitSpecialAttack {
            attacker_instance_id,
            target_instance_id,
            target_is_captain,
            ..
        } => (
            Some(attacker_instance_id.as_str()),
            Some(target_instance_id.as_str()),
            *target_is_captain == Some(true),
        ),
        GameAction::CaptainAttack {
            target_instance_id,
            target_is_captain,
            ..
        }
        | GameAction::UseSurcharge {
            target_instance_id,
            target_is_captain,
        } => (
            None,
            Some(target_instance_id.as_str()),
            *target_is_captain == Some(true),
        ),
        _ => (None, None, false),
    }
}

/// TS `scoreEvent(state, playerId, action)` — `src/engine/ai.ts:219`.
///
/// `gainWill` 18, `draw` 14, `healAlly` 8, `buffAllies` `12 + 2 × allies`,
/// `damageEnemies` `15 + 2 × amount`, `dodgeAll` 3, any other effect 6, and 5
/// when the card has no `eventEffect` at all.
pub fn score_event(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    action: &GameAction,
) -> EngineResult<f64> {
    let GameAction::PlayEvent { instance_id, .. } = action else {
        return Ok(0.0);
    };
    // TS `if (!card) return 0;` …
    let Some(card) = state.cards.get(instance_id) else {
        return Ok(0.0);
    };
    // … then `getCardDef(card.defId)`, which throws on an unknown definition.
    let def = registry.get_card_def(&card.def_id)?;
    let Some(effect) = def.event_effect.as_ref() else {
        return Ok(5.0);
    };

    Ok(match effect {
        // Very good early
        EventEffect::GainWill { .. } => 18.0,
        EventEffect::Draw { .. } => 14.0,
        EventEffect::HealAlly { .. } => 8.0,
        EventEffect::BuffAllies { .. } => {
            12.0 + get_board_characters(state, player_id).len() as f64 * 2.0
        }
        EventEffect::DamageEnemies { amount, .. } => 15.0 + *amount as f64 * 2.0,
        // Defensive, AI rarely needs
        EventEffect::DodgeAll => 3.0,
        _ => 6.0,
    })
}

/// TS `scoreShield(state, action)` — `src/engine/ai.ts:248`.
///
/// 32 when the block both saves a KO and the blocker survives, 20 when the
/// redirected damage is 0, 6 when it is strictly lower than the original and
/// the blocker survives, else 0.
pub fn score_shield(
    state: &GameState,
    registry: &CardRegistry,
    action: &GameAction,
) -> EngineResult<f64> {
    let GameAction::UseShield {
        blocker_instance_id,
    } = action
    else {
        return Ok(0.0);
    };
    let Some(pending) = state.pending_attack.as_ref() else {
        return Ok(0.0);
    };
    // TS calls `getEffectiveDef` *before* the `if (!blocker) return 0` guard
    // (ai.ts:254-256). That helper returns `0` for a missing instance rather
    // than throwing, which is exactly why the guard below is reachable; it
    // throws only for a *present* blocker (or attached object) whose `defId`
    // is unregistered.
    let blocker_def = get_effective_def(state, registry, blocker_instance_id)?;
    let Some(blocker) = state.cards.get(blocker_instance_id) else {
        return Ok(0.0);
    };
    let atk_power = pending.attack_power.unwrap_or(pending.raw_damage);
    let redirected = (atk_power - blocker_def).max(0);

    // Worthwhile if the block saves the original target from a KO and the blocker survives.
    let saves_ko = if !pending.target_is_captain {
        state
            .cards
            .get(&pending.target_id)
            .map(|c| c.current_pv)
            .unwrap_or(0)
            <= pending.raw_damage
    } else {
        // protecting the captain from a heavy hit
        pending.raw_damage >= 6
    };
    let blocker_survives = redirected < blocker.current_pv;

    if saves_ko && blocker_survives {
        return Ok(32.0);
    }
    if redirected == 0 {
        // fully negated, free
        return Ok(20.0);
    }
    if redirected < pending.raw_damage && blocker_survives {
        return Ok(6.0);
    }
    Ok(0.0)
}

/// TS `scoreCaptainFlip(state, playerId)` — `src/engine/ai.ts:272`.
///
/// −10 before T4; 30 at 0 allies from T5; 25 at ≤1 ally from T6; 15 from T7;
/// 10 from T5 with ≤2 allies; −5 otherwise.
pub fn score_captain_flip(state: &GameState, _registry: &CardRegistry, player_id: PlayerId) -> f64 {
    let board_count = get_board_characters(state, player_id).len();
    // Never flip before turn 4
    if state.turn_number < 4 {
        return -10.0;
    }
    // Flip when desperate (0-1 allies) and late game
    if board_count == 0 && state.turn_number >= 5 {
        return 30.0;
    }
    if board_count <= 1 && state.turn_number >= 6 {
        return 25.0;
    }
    if state.turn_number >= 7 {
        return 15.0;
    }
    if state.turn_number >= 5 && board_count <= 2 {
        return 10.0;
    }
    -5.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{marines_deck, mugiwara_deck};

    /// Test shim: the states below are all well-formed, so the (fallible)
    /// evaluation cannot throw — the TS failure path is covered by
    /// `choose_expert`'s `-Infinity` branch instead.
    fn evaluate_state(state: &GameState, registry: &CardRegistry, player_id: PlayerId) -> f64 {
        super::evaluate_state(state, registry, player_id).expect("valid state")
    }

    use crate::state::{PendingAttack, create_initial_state};
    use crate::types::{
        AtkDefStat, BaseAction, BuffDuration, CaptainDef, CaptainRecto, CaptainVerso, CardDef,
        DamageTarget, EntryEffect, FlipCondition, Modifier, ModifierDuration, PassiveDef,
        SpecialAttack, TurnDuration,
    };
    use crate::types::{CardType, Faction, Rarity, Slot, Zone};

    fn card(id: &str, ty: CardType) -> CardDef {
        CardDef::new(
            id,
            format!("Card {id}"),
            ty,
            2,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        )
    }

    /// A registry covering every def id the four decks reference plus the ids
    /// used by these tests. Real card data lives in `cards::*` (ported
    /// separately), so the AI tests stand on their own placeholders.
    fn registry() -> CardRegistry {
        let mut reg = CardRegistry::new();
        let mut ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for deck in crate::decks::all_decks() {
            for e in &deck.cards {
                ids.insert(e.card_id.clone());
            }
        }
        for id in ids {
            let mut d = card(&id, CardType::Character);
            d.atk = Some(2);
            d.def = Some(1);
            d.pv = Some(3);
            reg.register_card(d);
        }

        // Characters with distinct stats / preferred rows.
        let mut fighter = card("T-FIGHT", CardType::Character);
        fighter.atk = Some(4);
        fighter.def = Some(2);
        fighter.pv = Some(5);
        fighter.preferred_row = Some(Row::Front);
        reg.register_card(fighter);

        let mut support = card("T-SUPP", CardType::Character);
        support.atk = Some(1);
        support.def = Some(3);
        support.pv = Some(2);
        support.preferred_row = Some(Row::Back);
        reg.register_card(support);

        // Object with an ATK bonus.
        let mut sword = card("T-SWORD", CardType::Object);
        sword.bonus_atk = Some(3);
        reg.register_card(sword);

        // Events: one per scored effect family, one with none at all.
        let mut will = card("T-WILL", CardType::Event);
        will.event_effect = Some(EventEffect::GainWill { amount: 2 });
        reg.register_card(will);

        let mut buff = card("T-BUFF", CardType::Event);
        buff.event_effect = Some(EventEffect::BuffAllies {
            stat: AtkDefStat::Atk,
            amount: 1,
            filter: None,
            duration: BuffDuration::Turn,
        });
        reg.register_card(buff);

        let mut dmg = card("T-DMG", CardType::Event);
        dmg.event_effect = Some(EventEffect::DamageEnemies {
            amount: 3,
            target: DamageTarget::AllFront,
            cursed_bonus: None,
            sand: None,
            destroy_ships: None,
        });
        reg.register_card(dmg);

        let mut rally = card("T-RALLY", CardType::Event);
        rally.event_effect = Some(EventEffect::Rally {
            atk: 1,
            def: 1,
            heal: 1,
        });
        reg.register_card(rally);

        reg.register_card(card("T-PLAIN", CardType::Event));

        for id in ["CAP-LUFFY", "CAP-AKAINU", "CAP-CROCODILE", "CAP-SHANKS"] {
            reg.register_captain(test_captain(id));
        }
        reg
    }

    /// A minimal captain definition: the AI never reads captain stats beyond
    /// `currentPv`, so only the shape has to be valid.
    fn test_captain(id: &str) -> CaptainDef {
        let passive = PassiveDef {
            name: "cap".into(),
            description: "cap".into(),
            effects: Vec::new(),
        };
        CaptainDef {
            id: id.into(),
            name: id.into(),
            faction: Faction::Pirate,
            tags: None,
            traits: None,
            recto: CaptainRecto {
                pv: 20,
                atk: 3,
                def: 2,
                passive: passive.clone(),
                attacks: Vec::new(),
                surcharge: None,
            },
            flip_condition: FlipCondition {
                cost: Some(4),
                ..Default::default()
            },
            verso: CaptainVerso {
                pv: 25,
                atk: 5,
                def: 3,
                passive,
                entry_effect: EntryEffect::BuffAllies {
                    stat: AtkDefStat::Atk,
                    amount: 1,
                    duration: TurnDuration::Turn,
                },
                base_action: BaseAction {
                    name: "Gear".into(),
                    atk: 5,
                    ..Default::default()
                },
                special_attack: SpecialAttack {
                    name: "Special".into(),
                    cost: 3,
                    atk_bonus: 3,
                    ..Default::default()
                },
                surcharge: None,
                traits: None,
                natural_haki: None,
            },
        }
    }

    fn base_state(reg: &CardRegistry) -> GameState {
        let mut ctx = EngineContext::seeded(7);
        create_initial_state(&mugiwara_deck(), &marines_deck(), reg, &mut ctx).unwrap()
    }

    /// Put a fresh instance of `def_id` on `player`'s board in `slot`.
    fn place(state: &mut GameState, player: PlayerId, def_id: &str, slot: Slot, pv: i32) -> String {
        let id = format!("{def_id}#{slot:?}#{}", player.as_str());
        let mut inst = crate::state::CardInstance::new(id.clone(), def_id.into(), player, pv);
        inst.zone = Zone::Board;
        inst.slot = Some(slot);
        state.cards.insert(id.clone(), inst);
        state
            .players
            .get_mut(player)
            .board
            .set(slot, Some(id.clone()));
        id
    }

    /// Put a fresh instance of `def_id` into `player`'s hand.
    fn to_hand(state: &mut GameState, player: PlayerId, def_id: &str) -> String {
        let id = format!("{def_id}#hand#{}", player.as_str());
        let mut inst = crate::state::CardInstance::new(id.clone(), def_id.into(), player, 0);
        inst.zone = Zone::Hand;
        state.cards.insert(id.clone(), inst);
        state.players.get_mut(player).hand.push(id.clone());
        id
    }

    #[test]
    fn evaluate_state_propagates_the_get_card_def_throw() {
        // TS `getEffectiveAtk` returns 0 for a *missing instance*, but throws
        // from `getCardDef` when a board instance carries an unregistered
        // `defId` — and `evaluateState` does not catch it.
        let reg = registry();
        let mut state = base_state(&reg);
        place(&mut state, PlayerId::Player1, "T-GHOST", Slot::V1, 4);

        let err = super::evaluate_state(&state, &reg, PlayerId::Player1).unwrap_err();
        assert_eq!(err.to_string(), "Card not found: T-GHOST");

        // Inside `chooseExpert` that failure is the `-Infinity` branch, so the
        // candidate action loses to any action that scores at all.
        let mut ctx = EngineContext::seeded(1);
        let actions = [GameAction::EndTurn, GameAction::PassCounter];
        assert_eq!(
            choose_expert(&state, &reg, &mut ctx, PlayerId::Player1, &actions).unwrap(),
            GameAction::EndTurn
        );
    }

    #[test]
    fn ai_choose_action_propagates_the_get_valid_actions_throw() {
        // TS `aiChooseAction` calls `getValidActions` outside any try/catch,
        // so a hand id with no instance escapes as an exception instead of
        // silently shortening the action list.
        let reg = registry();
        let mut state = base_state(&reg);
        state.current_player = PlayerId::Player1;
        state.phase = crate::types::Phase::Main;
        state.players.player1.hand.push("nowhere".into());
        let mut ctx = EngineContext::seeded(2);

        let err = ai_choose_action(
            &state,
            &reg,
            &mut ctx,
            PlayerId::Player1,
            Difficulty::Intermediate,
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "Instance not found: nowhere");
    }

    #[test]
    fn evaluate_state_transcribes_every_ts_term() {
        let reg = registry();
        let mut state = base_state(&reg);
        // Clear hands so the hand terms are exactly controlled.
        state.players.player1.hand.clear();
        state.players.player2.hand.clear();

        // Empty board, equal captains, no volonte, no hands → 0.
        assert_eq!(evaluate_state(&state, &reg, PlayerId::Player1), 0.0);

        // Captain PV difference × 1.5
        state.players.player1.captain.current_pv = 18;
        state.players.player2.captain.current_pv = 12;
        assert_eq!(evaluate_state(&state, &reg, PlayerId::Player1), 9.0);
        assert_eq!(evaluate_state(&state, &reg, PlayerId::Player2), -9.0);

        // Own fighter: atk 4 + def 2 + 5×0.5 + 3 = 11.5
        place(&mut state, PlayerId::Player1, "T-FIGHT", Slot::V1, 5);
        assert_eq!(evaluate_state(&state, &reg, PlayerId::Player1), 9.0 + 11.5);
        // Enemy support: atk 1 + def 3 + 2×0.5 + 3 = 8
        place(&mut state, PlayerId::Player2, "T-SUPP", Slot::A1, 2);
        assert_eq!(
            evaluate_state(&state, &reg, PlayerId::Player1),
            9.0 + 11.5 - 8.0
        );

        // Hands (1.4 mine, 1.0 theirs) and Volonté (0.4).
        to_hand(&mut state, PlayerId::Player1, "T-WILL");
        to_hand(&mut state, PlayerId::Player1, "T-BUFF");
        to_hand(&mut state, PlayerId::Player2, "T-WILL");
        state.players.player1.volonte = 5;
        let expected = 9.0 + 11.5 - 8.0 + (2.0 * 1.4 - 1.0) + 2.0;
        assert!((evaluate_state(&state, &reg, PlayerId::Player1) - expected).abs() < 1e-9);

        // Dead captains and a decided winner are ±1000 each, and they stack.
        let mut dead = state.clone();
        dead.players.player2.captain.current_pv = 0;
        dead.winner = Some(PlayerId::Player1);
        let alive = evaluate_state(&state, &reg, PlayerId::Player1);
        let dead_val = evaluate_state(&dead, &reg, PlayerId::Player1);
        // +1000 (enemy captain down) +1000 (winner) plus the changed PV term.
        assert!((dead_val - (alive + 2000.0 + 18.0)).abs() < 1e-9);
        assert!(
            (evaluate_state(&dead, &reg, PlayerId::Player2)
                - (evaluate_state(&state, &reg, PlayerId::Player2) - 2000.0 - 18.0))
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn score_action_constants_match_ts() {
        let reg = registry();
        let state = base_state(&reg);
        let p = PlayerId::Player1;
        let s = |a: &GameAction| score_action(&state, &reg, p, a).unwrap();

        assert_eq!(
            s(&GameAction::DeployShip {
                instance_id: "x".into()
            }),
            15.0
        );
        assert_eq!(
            s(&GameAction::PlayCounter {
                instance_id: "x".into()
            }),
            50.0
        );
        assert_eq!(s(&GameAction::PassCounter), 0.0);
        assert_eq!(
            s(&GameAction::MoveCharacter {
                instance_id: "x".into(),
                target_slot: Slot::V1
            }),
            2.0
        );
        assert_eq!(s(&GameAction::EndTurn), -1.0);
        for (haki, want) in [
            (HakiType::Observation, 40.0),
            (HakiType::King, 60.0),
            (HakiType::Armament, 10.0),
        ] {
            assert_eq!(
                s(&GameAction::UseHaki {
                    haki_type: haki,
                    target_instance_id: None
                }),
                want
            );
        }
        // Action types absent from the TS switch fall through to 0.
        assert_eq!(
            s(&GameAction::AwakenFruit {
                fruit_instance_id: "x".into()
            }),
            0.0
        );
        assert_eq!(
            s(&GameAction::ActivateShip {
                ship_instance_id: "x".into()
            }),
            0.0
        );
        assert_eq!(
            s(&GameAction::BaseSupportAction {
                instance_id: "x".into(),
                target_instance_id: None
            }),
            0.0
        );
        // Decision §8.19 (was: `fruitSpecialAttack` is not in the TS switch, so
        // it scored 0 — below `moveCharacter`'s 2). It now goes through
        // `score_attack` and is scored as a special: 5 base + 5. The instances
        // here are unknown, so the KO block contributes nothing.
        assert_eq!(
            s(&GameAction::FruitSpecialAttack {
                attacker_instance_id: "x".into(),
                fruit_instance_id: "f".into(),
                target_instance_id: "t".into(),
                target_is_captain: None
            }),
            10.0
        );
    }

    /// Decision §8.19 — `fruitSpecialAttack` must outrank `moveCharacter`.
    #[test]
    fn fruit_special_attack_outscores_move_character() {
        let reg = registry();
        let mut state = base_state(&reg);
        let p = PlayerId::Player1;
        let attacker = place(&mut state, p, "T-FIGHT", Slot::V1, 5);
        let target = place(&mut state, PlayerId::Player2, "T-SUPP", Slot::V1, 1);
        let fruit = to_hand(&mut state, p, "T-SWORD");

        let fruit_attack = GameAction::FruitSpecialAttack {
            attacker_instance_id: attacker.clone(),
            fruit_instance_id: fruit,
            target_instance_id: target,
            target_is_captain: None,
        };
        let mv = GameAction::MoveCharacter {
            instance_id: attacker,
            target_slot: Slot::V2,
        };
        // 5 base + 20 KO (atk 4 − def 3 = 1 ≥ 1 PV) + max(0, 5 − 1) + 5 special.
        assert_eq!(score_action(&state, &reg, p, &fruit_attack).unwrap(), 34.0);
        assert!(
            score_action(&state, &reg, p, &fruit_attack).unwrap()
                > score_action(&state, &reg, p, &mv).unwrap()
        );
    }

    #[test]
    fn score_deploy_and_equip_match_ts() {
        let reg = registry();
        let mut state = base_state(&reg);
        let fighter = to_hand(&mut state, PlayerId::Player1, "T-FIGHT");
        let support = to_hand(&mut state, PlayerId::Player1, "T-SUPP");

        // Empty board: 10 + 2×4 + 5 + 10 + 20 = 53, +3 for the matching row.
        let a = GameAction::DeployCharacter {
            instance_id: fighter.clone(),
            slot: Slot::V2,
        };
        assert_eq!(
            score_deploy_character(&state, &reg, PlayerId::Player1, &a).unwrap(),
            56.0
        );
        let back = GameAction::DeployCharacter {
            instance_id: fighter.clone(),
            slot: Slot::A2,
        };
        assert_eq!(
            score_deploy_character(&state, &reg, PlayerId::Player1, &back).unwrap(),
            53.0
        );

        // Support prefers the back row: 10 + 2 + 2 + 10 + 20 + 3 = 47.
        let sa = GameAction::DeployCharacter {
            instance_id: support,
            slot: Slot::A1,
        };
        assert_eq!(
            score_deploy_character(&state, &reg, PlayerId::Player1, &sa).unwrap(),
            47.0
        );

        // With three allies the two "few characters" bonuses are gone: 53 − 30 = 26 (+3 row).
        place(&mut state, PlayerId::Player1, "T-SUPP", Slot::A1, 2);
        place(&mut state, PlayerId::Player1, "T-SUPP", Slot::A2, 2);
        place(&mut state, PlayerId::Player1, "T-SUPP", Slot::A3, 2);
        assert_eq!(
            score_deploy_character(&state, &reg, PlayerId::Player1, &a).unwrap(),
            26.0
        );

        // Unknown instance → 0.
        let ghost = GameAction::DeployCharacter {
            instance_id: "nope".into(),
            slot: Slot::V1,
        };
        assert_eq!(
            score_deploy_character(&state, &reg, PlayerId::Player1, &ghost).unwrap(),
            0.0
        );

        // equipObject: 8 + 3 × bonusAtk
        let sword = to_hand(&mut state, PlayerId::Player1, "T-SWORD");
        let eq = GameAction::EquipObject {
            object_instance_id: sword,
            target_instance_id: fighter,
            target_is_captain: None,
        };
        assert_eq!(score_equip_object(&state, &reg, &eq).unwrap(), 17.0);
    }

    /// Decision §8.20 — a dangling `equipObject` scores 0 and, because
    /// `score_action` runs outside `choose_expert`'s try/catch, the chooser
    /// still returns an action instead of aborting the AI turn.
    #[test]
    fn dangling_equip_object_scores_zero_and_never_aborts_the_chooser() {
        let reg = registry();
        let state = base_state(&reg);
        let p = PlayerId::Player1;
        let equip = GameAction::EquipObject {
            object_instance_id: "nowhere".into(),
            target_instance_id: "nowhere-either".into(),
            target_is_captain: None,
        };
        assert_eq!(score_action(&state, &reg, p, &equip).unwrap(), 0.0);

        let mut ctx = EngineContext::seeded(11);
        let actions = [equip, GameAction::EndTurn];
        assert_eq!(
            choose_expert(&state, &reg, &mut ctx, p, &actions).unwrap(),
            GameAction::EndTurn
        );
        assert_eq!(
            choose_by_score(&state, &reg, &mut ctx, p, &actions, 0.0).unwrap(),
            actions[0]
        );
    }

    #[test]
    fn score_attack_ko_block_skips_captain_attacks() {
        let reg = registry();
        let mut state = base_state(&reg);
        let attacker = place(&mut state, PlayerId::Player1, "T-FIGHT", Slot::V1, 5);
        // Target: def 3, pv 2 → damage max(0, 4 − 3) = 1 < 2, no KO bonus,
        // but max(0, 5 − 2) = 3 for the low-PV term.
        let target = place(&mut state, PlayerId::Player2, "T-SUPP", Slot::V1, 2);
        let base = GameAction::BaseAttack {
            attacker_instance_id: attacker.clone(),
            target_instance_id: target.clone(),
            target_is_captain: None,
        };
        assert_eq!(
            score_attack(&state, &reg, PlayerId::Player1, &base).unwrap(),
            8.0
        );

        // A 1-PV target dies to the same hit: +20.
        state.cards.get_mut(&target).unwrap().current_pv = 1;
        assert_eq!(
            score_attack(&state, &reg, PlayerId::Player1, &base).unwrap(),
            5.0 + 20.0 + 4.0
        );

        // specialAttack adds 5 on top.
        let special = GameAction::SpecialAttack {
            attacker_instance_id: attacker.clone(),
            target_instance_id: target.clone(),
            target_is_captain: None,
        };
        assert_eq!(
            score_attack(&state, &reg, PlayerId::Player1, &special).unwrap(),
            5.0 + 20.0 + 4.0 + 5.0
        );

        // Targeting the captain: flat 5 + 15, KO block skipped.
        let at_captain = GameAction::BaseAttack {
            attacker_instance_id: attacker,
            target_instance_id: target.clone(),
            target_is_captain: Some(true),
        };
        assert_eq!(
            score_attack(&state, &reg, PlayerId::Player1, &at_captain).unwrap(),
            20.0
        );

        // Decision §8.19 (was: captainAttack carries no attackerInstanceId, so
        // the whole KO block was skipped even against a character and it scored
        // a flat 5). The attacker's ATK now comes from the captain's *active*
        // face: recto atk 3 − target def 3 = 0 damage, so no KO bonus yet, but
        // the low-PV term does apply — 5 + max(0, 5 − 1) = 9.
        let cap = GameAction::CaptainAttack {
            target_instance_id: target.clone(),
            target_is_captain: None,
            is_special: None,
        };
        assert_eq!(
            score_attack(&state, &reg, PlayerId::Player1, &cap).unwrap(),
            9.0
        );
        // Flipped: the verso's atk 5 − def 3 = 2 ≥ 1 PV → the KO bonus lands.
        state.players.player1.captain.flipped = true;
        assert_eq!(
            score_attack(&state, &reg, PlayerId::Player1, &cap).unwrap(),
            5.0 + 20.0 + 4.0
        );
        // ATK modifiers on the captain count too — and so does `useSurcharge`,
        // which resolves through the same captain face.
        state.players.player1.captain.flipped = false;
        state.players.player1.captain.modifiers.push(Modifier {
            id: "test-atk".into(),
            stat: ModifierStat::Atk,
            amount: 2,
            source: "test".into(),
            duration: ModifierDuration::Permanent,
            turns_remaining: None,
        });
        let surcharge = GameAction::UseSurcharge {
            target_instance_id: target.clone(),
            target_is_captain: None,
        };
        assert_eq!(
            score_attack(&state, &reg, PlayerId::Player1, &surcharge).unwrap(),
            5.0 + 20.0 + 4.0
        );
        state.players.player1.captain.modifiers.clear();

        // Against a captain the KO block is still skipped: 5 + 15.
        let cap_on_cap = GameAction::CaptainAttack {
            target_instance_id: target,
            target_is_captain: Some(true),
            is_special: Some(true),
        };
        assert_eq!(
            score_attack(&state, &reg, PlayerId::Player1, &cap_on_cap).unwrap(),
            20.0
        );
    }

    /// Decision §8.19 — the intermediate AI must see a lethal captain swing.
    #[test]
    fn intermediate_ai_takes_the_lethal_captain_attack() {
        let reg = registry();
        let mut state = base_state(&reg);
        let p = PlayerId::Player1;
        let fighter = place(&mut state, p, "T-FIGHT", Slot::V1, 5);
        // Lethal for the flipped captain (atk 5 − def 3 = 2 ≥ 1 PV).
        let dying = place(&mut state, PlayerId::Player2, "T-SUPP", Slot::V1, 1);
        // Survives the fighter (atk 4 − def 3 = 1 < 4 PV): that attack scores
        // 5 + max(0, 5 − 4) = 6, which used to beat the captain's flat 5.
        let healthy = place(&mut state, PlayerId::Player2, "T-SUPP", Slot::V2, 4);
        state.players.player1.captain.flipped = true;

        let cap = GameAction::CaptainAttack {
            target_instance_id: dying,
            target_is_captain: None,
            is_special: None,
        };
        let actions = [
            GameAction::BaseAttack {
                attacker_instance_id: fighter,
                target_instance_id: healthy,
                target_is_captain: None,
            },
            cap.clone(),
            GameAction::EndTurn,
        ];
        let mut ctx = EngineContext::seeded(3);
        assert_eq!(
            choose_by_score(&state, &reg, &mut ctx, p, &actions, 0.0).unwrap(),
            cap
        );
    }

    #[test]
    fn score_event_matches_ts_table() {
        let reg = registry();
        let mut state = base_state(&reg);
        let p = PlayerId::Player1;
        let ev = |state: &GameState, id: &str| {
            score_event(
                state,
                &reg,
                p,
                &GameAction::PlayEvent {
                    instance_id: id.into(),
                    targets: None,
                },
            )
            .unwrap()
        };

        let will = to_hand(&mut state, p, "T-WILL");
        let buff = to_hand(&mut state, p, "T-BUFF");
        let dmg = to_hand(&mut state, p, "T-DMG");
        let rally = to_hand(&mut state, p, "T-RALLY");
        let plain = to_hand(&mut state, p, "T-PLAIN");

        assert_eq!(ev(&state, &will), 18.0);
        assert_eq!(ev(&state, &buff), 12.0); // no allies yet
        assert_eq!(ev(&state, &dmg), 15.0 + 6.0);
        assert_eq!(ev(&state, &rally), 6.0); // not in the switch → default
        assert_eq!(ev(&state, &plain), 5.0); // no eventEffect at all
        assert_eq!(ev(&state, "missing"), 0.0);

        place(&mut state, p, "T-SUPP", Slot::A1, 2);
        place(&mut state, p, "T-SUPP", Slot::A2, 2);
        assert_eq!(ev(&state, &buff), 12.0 + 4.0);
    }

    #[test]
    fn score_shield_branches_match_ts() {
        let reg = registry();
        let mut state = base_state(&reg);
        let blocker = place(&mut state, PlayerId::Player1, "T-SUPP", Slot::V2, 2); // def 3
        let victim = place(&mut state, PlayerId::Player1, "T-FIGHT", Slot::V1, 2);
        let act = GameAction::UseShield {
            blocker_instance_id: blocker.clone(),
        };

        // No pending attack → 0.
        assert_eq!(score_shield(&state, &reg, &act).unwrap(), 0.0);

        let pending = |power: i32, raw: i32, target: &str, is_cap: bool| PendingAttack {
            attacker_id: "enemy".into(),
            target_id: target.into(),
            target_is_captain: is_cap,
            is_special: false,
            raw_damage: raw,
            attack_power: Some(power),
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
        };

        // Saves a KO (victim pv 2 ≤ raw 3) and the blocker survives
        // (redirected max(0, 4 − 3) = 1 < 2) → 32.
        state.pending_attack = Some(pending(4, 3, &victim, false));
        assert_eq!(score_shield(&state, &reg, &act).unwrap(), 32.0);

        // Fully negated (power 3 − def 3 = 0) but the victim survives (pv 9) → 20.
        state.cards.get_mut(&victim).unwrap().current_pv = 9;
        state.pending_attack = Some(pending(3, 3, &victim, false));
        assert_eq!(score_shield(&state, &reg, &act).unwrap(), 20.0);

        // Partial soak: redirected 1 < raw 5, blocker survives → 6.
        state.pending_attack = Some(pending(4, 5, &victim, false));
        assert_eq!(score_shield(&state, &reg, &act).unwrap(), 6.0);

        // Blocker dies and nothing is saved → 0.
        state.cards.get_mut(&blocker).unwrap().current_pv = 1;
        state.pending_attack = Some(pending(4, 5, &victim, false));
        assert_eq!(score_shield(&state, &reg, &act).unwrap(), 0.0);
        state.cards.get_mut(&blocker).unwrap().current_pv = 2;

        // Captain target: savesKO is `rawDamage >= 6`, not a PV lookup.
        state.pending_attack = Some(pending(4, 6, "captain-placeholder", true));
        assert_eq!(score_shield(&state, &reg, &act).unwrap(), 32.0);
        state.pending_attack = Some(pending(4, 5, "captain-placeholder", true));
        assert_eq!(score_shield(&state, &reg, &act).unwrap(), 6.0);

        // attackPower is optional — it falls back to rawDamage.
        let mut p = pending(0, 5, &victim, false);
        p.attack_power = None;
        state.pending_attack = Some(p);
        // redirected = max(0, 5 − 3) = 2, blocker pv 2 → does not survive,
        // victim pv 9 > 5 → no KO saved → 0.
        assert_eq!(score_shield(&state, &reg, &act).unwrap(), 0.0);
    }

    #[test]
    fn score_captain_flip_table_matches_ts() {
        let reg = registry();
        let mut state = base_state(&reg);
        let p = PlayerId::Player1;

        for turn in 1..4 {
            state.turn_number = turn;
            assert_eq!(score_captain_flip(&state, &reg, p), -10.0);
        }
        // T4, empty board → falls through every late-game gate → −5.
        state.turn_number = 4;
        assert_eq!(score_captain_flip(&state, &reg, p), -5.0);
        // T5, 0 allies → 30.
        state.turn_number = 5;
        assert_eq!(score_captain_flip(&state, &reg, p), 30.0);
        // T5 with one ally: not the 0-ally branch, not yet T6 → the ≤2 branch.
        place(&mut state, p, "T-SUPP", Slot::A1, 2);
        assert_eq!(score_captain_flip(&state, &reg, p), 10.0);
        // T6 with one ally → 25.
        state.turn_number = 6;
        assert_eq!(score_captain_flip(&state, &reg, p), 25.0);
        // T6 with three allies → −5 (nothing matches).
        place(&mut state, p, "T-SUPP", Slot::A2, 2);
        place(&mut state, p, "T-SUPP", Slot::A3, 2);
        assert_eq!(score_captain_flip(&state, &reg, p), -5.0);
        // T7 with three allies → 15.
        state.turn_number = 7;
        assert_eq!(score_captain_flip(&state, &reg, p), 15.0);
    }

    #[test]
    fn choose_by_score_keeps_the_first_of_equal_scores() {
        let reg = registry();
        let state = base_state(&reg);
        let mut ctx = EngineContext::seeded(3);
        let a = GameAction::DeployShip {
            instance_id: "a".into(),
        };
        let b = GameAction::DeployShip {
            instance_id: "b".into(),
        };
        // Both score 15; strict `>` keeps the first.
        let picked = choose_by_score(
            &state,
            &reg,
            &mut ctx,
            PlayerId::Player1,
            &[a.clone(), b],
            0.0,
        )
        .unwrap();
        assert_eq!(picked, a);
        // Zero jitter must not touch the RNG.
        assert_eq!(ctx, EngineContext::seeded(3));

        // The best action wins regardless of position.
        let picked = choose_by_score(
            &state,
            &reg,
            &mut ctx,
            PlayerId::Player1,
            &[
                GameAction::EndTurn,
                GameAction::PlayCounter {
                    instance_id: "c".into(),
                },
                GameAction::PassCounter,
            ],
            0.0,
        )
        .unwrap();
        assert_eq!(
            picked,
            GameAction::PlayCounter {
                instance_id: "c".into()
            }
        );

        // With jitter every action draws exactly once, in order.
        let mut jittered = EngineContext::seeded(3);
        choose_by_score(
            &state,
            &reg,
            &mut jittered,
            PlayerId::Player1,
            &[a.clone(), a.clone(), a],
            7.0,
        )
        .unwrap();
        let mut manual = EngineContext::seeded(3);
        for _ in 0..3 {
            manual.rng.random_f64();
        }
        assert_eq!(jittered, manual);
    }

    #[test]
    fn beginner_only_draws_for_the_pass_when_a_pass_exists() {
        let reg = registry();
        let mut state = base_state(&reg);
        let actions = [
            GameAction::DeployShip {
                instance_id: "a".into(),
            },
            GameAction::EndTurn,
        ];

        // No pending attack → the defence draw is skipped entirely.
        let mut ctx = EngineContext::seeded(11);
        let _ = choose_beginner(&state, &reg, &mut ctx, PlayerId::Player1, &actions).unwrap();
        let mut expected = EngineContext::seeded(11);
        let roll = expected.rng.random_f64();
        if roll < 0.35 {
            expected.rng.random_index(1); // only `endTurn` is in BASIC
        } else {
            expected.rng.random_f64(); // one jitter draw for the single-item pool
        }
        assert_eq!(ctx, expected);

        // A pending attack with no passCounter on offer also skips the draw
        // (`pass && Math.random() < 0.8` short-circuits).
        state.pending_attack = Some(PendingAttack {
            attacker_id: "x".into(),
            target_id: "y".into(),
            target_is_captain: false,
            is_special: false,
            raw_damage: 1,
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
        let mut ctx2 = EngineContext::seeded(11);
        let _ = choose_beginner(&state, &reg, &mut ctx2, PlayerId::Player1, &actions).unwrap();
        assert_eq!(ctx2, ctx);

        // With a passCounter available, the 0.8 draw happens first.
        let with_pass = [
            GameAction::PassCounter,
            GameAction::PlayCounter {
                instance_id: "c".into(),
            },
        ];
        let mut ctx3 = EngineContext::seeded(11);
        let picked =
            choose_beginner(&state, &reg, &mut ctx3, PlayerId::Player1, &with_pass).unwrap();
        let mut probe = EngineContext::seeded(11);
        if probe.rng.random_f64() < 0.8 {
            assert_eq!(picked, GameAction::PassCounter);
            assert_eq!(ctx3, probe);
        }
    }

    #[test]
    fn beginner_falls_back_to_the_full_pool_without_basic_actions() {
        let reg = registry();
        let state = base_state(&reg);
        // Neither type is in BASIC, so `simple` is empty and the pool is `actions`.
        let actions = [
            GameAction::DeployShip {
                instance_id: "a".into(),
            },
            GameAction::UseHaki {
                haki_type: HakiType::King,
                target_instance_id: None,
            },
        ];
        for seed in 0..8u64 {
            let mut ctx = EngineContext::seeded(seed);
            let picked =
                choose_beginner(&state, &reg, &mut ctx, PlayerId::Player1, &actions).unwrap();
            assert!(actions.contains(&picked));
        }
    }

    #[test]
    fn scoring_unknown_data_propagates_the_ts_throw() {
        let reg = registry();
        let p = PlayerId::Player1;
        let mut state = base_state(&reg);
        // An instance whose definition is not registered: every TS helper below
        // reaches `getCardDef` and throws.
        let ghost = "GHOST#1".to_string();
        let mut inst = crate::state::CardInstance::new(ghost.clone(), "GHOST".into(), p, 1);
        inst.zone = Zone::Hand;
        state.cards.insert(ghost.clone(), inst);

        let deploy = GameAction::DeployCharacter {
            instance_id: ghost.clone(),
            slot: Slot::V1,
        };
        assert_eq!(
            score_action(&state, &reg, p, &deploy)
                .unwrap_err()
                .to_string(),
            "Card not found: GHOST"
        );
        assert!(
            score_action(
                &state,
                &reg,
                p,
                &GameAction::PlayEvent {
                    instance_id: ghost.clone(),
                    targets: None,
                }
            )
            .is_err()
        );
        // Decision §8.20 (was: `is_err()` — the TS `getCardDef` throw): an
        // object whose definition is unknown scores 0 instead of aborting the
        // AI turn from outside `chooseExpert`'s try/catch.
        assert_eq!(
            score_action(
                &state,
                &reg,
                p,
                &GameAction::EquipObject {
                    object_instance_id: ghost.clone(),
                    target_instance_id: ghost.clone(),
                    target_is_captain: None,
                }
            )
            .unwrap(),
            0.0
        );
        assert!(
            score_action(
                &state,
                &reg,
                p,
                &GameAction::BaseAttack {
                    attacker_instance_id: ghost.clone(),
                    target_instance_id: ghost.clone(),
                    target_is_captain: None,
                }
            )
            .is_err()
        );
        // Decision §8.20 (was: `is_err()` — the TS TypeError on
        // `state.cards[objectInstanceId].defId`): a missing instance scores 0.
        assert_eq!(
            score_equip_object(
                &state,
                &reg,
                &GameAction::EquipObject {
                    object_instance_id: "nowhere".into(),
                    target_instance_id: ghost.clone(),
                    target_is_captain: None,
                }
            )
            .unwrap(),
            0.0
        );

        // `scoreAction` sits outside `chooseExpert`'s try/catch (ai.ts:70), so
        // the throw escapes the chooser rather than scoring −Infinity.
        let mut ctx = EngineContext::seeded(1);
        let one = std::slice::from_ref(&deploy);
        assert!(choose_expert(&state, &reg, &mut ctx, p, one).is_err());
        assert!(choose_by_score(&state, &reg, &mut ctx, p, one, 0.0).is_err());
    }

    #[test]
    fn expert_rollouts_advance_the_shared_context_like_the_ts_module_globals() {
        let mut reg = registry();
        // A token and the event that spawns it — `generateInstanceId` bumps the
        // TS module-global `instanceCounter`, which no `catch` ever restores.
        let mut tok = card("T-TOK", CardType::Character);
        tok.atk = Some(1);
        tok.def = Some(0);
        tok.pv = Some(1);
        tok.is_token = Some(true);
        reg.register_card(tok);
        let mut spawner = card("T-SPAWN", CardType::Event);
        spawner.event_effect = Some(EventEffect::DeployTokens {
            token_id: "T-TOK".into(),
            count: 1,
        });
        reg.register_card(spawner);

        let p = PlayerId::Player1;
        let mut state = base_state(&reg);
        state.current_player = p;
        state.players.get_mut(p).volonte = 10;
        let iid = "T-SPAWN#1".to_string();
        let mut inst = crate::state::CardInstance::new(iid.clone(), "T-SPAWN".into(), p, 0);
        inst.zone = Zone::Hand;
        state.cards.insert(iid.clone(), inst);
        state.players.get_mut(p).hand.push(iid.clone());

        let actions = [
            GameAction::PlayEvent {
                instance_id: iid,
                targets: None,
            },
            GameAction::EndTurn,
        ];
        let mut ctx = EngineContext::seeded(5);
        let before = ctx.clone();
        let state_before = state.clone();
        choose_expert(&state, &reg, &mut ctx, p, &actions).unwrap();

        // TS `chooseExpert` passes no context: `executeAction` writes straight
        // to the module globals, so the speculative token spawn is still
        // counted when the chosen action is finally committed.
        assert!(
            ctx.instance_counter > before.instance_counter,
            "the rollout must advance the shared instanceCounter"
        );
        // The state itself is untouched — TS builds a new one per rollout.
        assert_eq!(state, state_before);
    }

    #[test]
    fn expert_penalises_end_turn_and_scores_failures_as_negative_infinity() {
        let reg = registry();
        let state = base_state(&reg);
        let p = PlayerId::Player1;

        // An illegal move errors out (the TS `catch`) → −Infinity, so `endTurn`
        // wins even with its −6 penalty.
        let actions = [
            GameAction::MoveCharacter {
                instance_id: "does-not-exist".into(),
                target_slot: Slot::V1,
            },
            GameAction::EndTurn,
        ];
        let mut ctx = EngineContext::seeded(5);
        let before = ctx.clone();
        let picked = choose_expert(&state, &reg, &mut ctx, p, &actions).unwrap();
        assert_eq!(picked, GameAction::EndTurn);
        // Neither rollout touches `Math.random()` or `instanceCounter`, so the
        // shared context happens to come out unchanged here.
        assert_eq!(ctx, before);
        // The real state is never mutated (TS `executeAction` is pure).
        let fresh = base_state(&reg);
        assert_eq!(state.turn_number, fresh.turn_number);
        assert_eq!(state.current_player, fresh.current_player);

        // Two failing actions: the first one wins (strict `>` never fires).
        let both_bad = [
            GameAction::MoveCharacter {
                instance_id: "nope-1".into(),
                target_slot: Slot::V1,
            },
            GameAction::MoveCharacter {
                instance_id: "nope-2".into(),
                target_slot: Slot::V2,
            },
        ];
        let picked = choose_expert(&state, &reg, &mut ctx, p, &both_bad).unwrap();
        assert_eq!(picked, both_bad[0]);
    }

    #[test]
    fn difficulty_serialises_with_the_ts_literals() {
        for (d, lit) in [
            (Difficulty::Beginner, "\"beginner\""),
            (Difficulty::Intermediate, "\"intermediate\""),
            (Difficulty::Expert, "\"expert\""),
        ] {
            assert_eq!(serde_json::to_string(&d).unwrap(), lit);
            assert_eq!(serde_json::from_str::<Difficulty>(lit).unwrap(), d);
        }
        assert_eq!(Difficulty::default(), Difficulty::Intermediate);
        assert_eq!(
            BASIC_ACTION_TYPES,
            [
                "deployCharacter",
                "baseAttack",
                "moveCharacter",
                "endTurn",
                "passCounter",
                "playCounter",
                "useShield"
            ]
        );
    }
}

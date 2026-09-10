//! Passive effects — port of `src/engine/passives.ts`
//! (`applyStartOfTurnPassives`, `recalculatePassiveBuffs`,
//! `applyEnemyDebuffAuras`, `applyOnKOEffects`, `matchesFilter`).
//!
//! The TS code reads `getCardDef` / `getCaptainDef` from the global registry
//! and `Date.now()` for a few modifier ids; here the [`CardRegistry`] and the
//! [`EngineContext`] (clock) are passed explicitly.

#![allow(clippy::collapsible_if)]
// ^ The nested `if` / `if let` blocks in this module mirror the TypeScript
// source branch for branch (see the per-function `PORT:` references). Merging
// them into let-chains would break that 1:1 reading, which is the whole point
// of the port, so the lint is turned off for this file only.

use crate::board::{
    get_board_characters, get_effective_atk, heal_unit, remove_from_board, ship_passive_scope,
};
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::registry::CardRegistry;
use crate::state::GameState;
use crate::types::{
    AllyFilter, AtkDefStat, AtkStat, BuffStat, Modifier, ModifierDuration, ModifierStat,
    OnAllyKoEffectKind, PassiveEffect, PlayerId, Slot, Zone,
};

/// TS `"atk" | "def" | "pv"` → `Modifier.stat`.
fn buff_stat(stat: BuffStat) -> ModifierStat {
    match stat {
        BuffStat::Atk => ModifierStat::Atk,
        BuffStat::Def => ModifierStat::Def,
        BuffStat::Pv => ModifierStat::Pv,
    }
}

/// The exact TS literal of a `"atk" | "def" | "pv"` stat (used in modifier ids).
fn buff_stat_str(stat: BuffStat) -> &'static str {
    match stat {
        BuffStat::Atk => "atk",
        BuffStat::Def => "def",
        BuffStat::Pv => "pv",
    }
}

/// TS `"atk" | "def"` → `Modifier.stat`.
fn atk_def_stat(stat: AtkDefStat) -> ModifierStat {
    match stat {
        AtkDefStat::Atk => ModifierStat::Atk,
        AtkDefStat::Def => ModifierStat::Def,
    }
}

/// TS `effect.stat.toUpperCase()` for an `"atk" | "def"` stat.
fn atk_def_stat_upper(stat: AtkDefStat) -> &'static str {
    match stat {
        AtkDefStat::Atk => "ATK",
        AtkDefStat::Def => "DEF",
    }
}

/// TS `"atk"` (`selfBuffOnAllyKO.stat`) → `Modifier.stat`.
fn atk_stat(stat: AtkStat) -> ModifierStat {
    match stat {
        AtkStat::Atk => ModifierStat::Atk,
    }
}

// ============================================================
// Start-of-turn passive effects
// ============================================================

/// TS `applyStartOfTurnPassives(state, playerId)` — every board character's
/// passive effects in slot order, then the captain's (a no-op).
pub fn apply_start_of_turn_passives(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    player_id: PlayerId,
) -> Result<(), EngineError> {
    // TS reads `state.players[playerId]` / `state.cards[charId]` from the
    // state *as it was on entry* — snapshot the (id, slot, effects) triples.
    let mut sources: Vec<(String, Slot, Vec<PassiveEffect>)> = Vec::new();
    {
        let player = state.players.get(player_id);
        for slot in Slot::ALL {
            let Some(char_id) = player.board.get(slot) else {
                continue;
            };
            let Some(card) = state.cards.get(char_id) else {
                continue;
            };
            let def = registry.get_card_def(&card.def_id)?;
            let Some(passive) = &def.passive else {
                continue;
            };
            sources.push((char_id.clone(), slot, passive.effects.clone()));
        }
    }

    for (char_id, slot, effects) in sources {
        for effect in effects {
            resolve_start_of_turn_effect(state, registry, ctx, player_id, &char_id, slot, &effect)?;
        }
    }

    // Process captain passives (recto or verso)
    apply_captain_start_of_turn(state, player_id);
    Ok(())
}

/// TS `resolveStartOfTurnEffect` — `healAdjacent` and `startTurnBuffAlly`;
/// every other passive type is continuous and handled elsewhere.
fn resolve_start_of_turn_effect(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    player_id: PlayerId,
    source_id: &str,
    source_slot: Slot,
    effect: &PassiveEffect,
) -> Result<(), EngineError> {
    match effect {
        PassiveEffect::HealAdjacent { amount } => {
            let amount = *amount;
            let adjacent = source_slot.adjacency();
            let source_name = registry
                .get_card_def(&state.get_card(source_id)?.def_id)?
                .name
                .clone();
            let adj_ids: Vec<String> = adjacent
                .iter()
                .filter_map(|s| state.players.get(player_id).board.get(*s).cloned())
                .collect();
            for adj_id in adj_ids {
                // Decision §8.5: the single heal path — never lowers PV and
                // skips a `noHeal` / `desiccation` unit.
                heal_unit(state, registry, &adj_id, amount)?;
            }
            state.add_log(
                player_id,
                format!("{source_name} soigne {amount} PV aux adjacents"),
            );
            Ok(())
        }
        PassiveEffect::StartTurnBuffAlly { stat, amount } => {
            // Usopp Vantardise: give one ally (highest ATK, not the source) +amount this turn.
            let source_name = registry
                .get_card_def(&state.get_card(source_id)?.def_id)?
                .name
                .clone();
            let mut best_id: Option<String> = None;
            let mut best_atk = -1;
            for slot in Slot::ALL {
                let Some(id) = state.players.get(player_id).board.get(slot) else {
                    continue;
                };
                if id == source_id {
                    continue;
                }
                let a = get_effective_atk(state, registry, id)?;
                if a > best_atk {
                    best_atk = a;
                    best_id = Some(id.clone());
                }
            }
            let Some(target_id) = best_id else {
                return Ok(());
            };
            let (stat, amount) = (*stat, *amount);
            state.get_card_mut(&target_id)?.modifiers.push(Modifier {
                id: format!("vantardise_{target_id}_{}", ctx.now()),
                stat: atk_def_stat(stat),
                amount,
                // Decision §8.1-2: the buff lasts the whole turn, so it must not
                // share the `passive_` prefix that `recalculate_passive_buffs`
                // strips on every deploy/equip/KO/flip. It still expires as a
                // `Turn` modifier in `reset_turn_flags`.
                source: format!("vantardise_{source_id}"),
                duration: ModifierDuration::Turn,
                turns_remaining: None,
            });
            state.add_log(
                player_id,
                format!(
                    "{source_name} : Vantardise — un allié gagne +{amount} {} ce tour.",
                    atk_def_stat_upper(stat)
                ),
            );
            Ok(())
        }
        _ => Ok(()),
    }
}

/// TS `applyCaptainStartOfTurn` — captain passives that trigger at start of
/// turn are rare; most are continuous (`recalculatePassiveBuffs`). No-op.
fn apply_captain_start_of_turn(_state: &mut GameState, _player_id: PlayerId) {}

// ============================================================
// Recalculate all passive buffs (continuous effects)
// Called after deploy, KO, equip, captain flip
// ============================================================

/// A board character as collected by TS `recalculatePassiveBuffs`.
struct BoardChar {
    id: String,
    def_id: String,
}

/// TS `desc.match(/\+(\d+)\s*<keyword>/)` on an already lower-cased ship
/// passive: the first `+<digits>` followed by optional whitespace and
/// `keyword`. `parseInt` of the captured digits.
fn ship_bonus(desc: &str, keyword: &str) -> i32 {
    let bytes = desc.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > start {
                let rest = &desc[end..];
                let rest = rest.trim_start_matches(crate::board::is_js_space);
                if rest.starts_with(keyword) {
                    // TS `parseInt` returns a double, so a digit run wider than
                    // `i32` still yields a finite (large) bonus, never 0.
                    return crate::board::js_parse_int(&desc[start..end]);
                }
            }
        }
        i += 1;
    }
    0
}

/// TS `recalculatePassiveBuffs(state, playerId)` — strip every
/// `passive_*` / `captain_*` / `synergy_*` modifier from the player's board
/// characters, then reapply captain `buffAlly`, character `buffAlly`, ship
/// passives and synergy bonuses.
pub fn recalculate_passive_buffs(
    state: &mut GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
) -> Result<(), EngineError> {
    let board_ids: Vec<String> = state
        .players
        .get(player_id)
        .board
        .instance_ids()
        .into_iter()
        .cloned()
        .collect();

    // Remove all passive-source modifiers from all board characters
    for id in &board_ids {
        if let Some(card) = state.cards.get_mut(id) {
            card.modifiers.retain(|m| {
                !m.source.starts_with("passive_")
                    && !m.source.starts_with("captain_")
                    && !m.source.starts_with("synergy_")
            });
        }
    }

    // Collect all board characters
    let board_chars: Vec<BoardChar> = board_ids
        .iter()
        .filter_map(|id| {
            state.cards.get(id).map(|c| BoardChar {
                id: id.clone(),
                def_id: c.def_id.clone(),
            })
        })
        .collect();

    // === Captain passive buffs ===
    {
        let captain = &state.players.get(player_id).captain;
        let cap_def = registry.get_captain_def(&captain.def_id)?;
        let cap_passive = if captain.flipped {
            &cap_def.verso.passive
        } else {
            &cap_def.recto.passive
        };
        let cap_id = cap_def.id.clone();
        let mut pushes: Vec<(String, Modifier)> = Vec::new();
        for effect in &cap_passive.effects {
            if let PassiveEffect::BuffAlly {
                stat,
                amount,
                filter,
            } = effect
            {
                for ch in &board_chars {
                    if matches_filter(registry, &ch.def_id, filter.as_ref())? {
                        pushes.push((
                            ch.id.clone(),
                            Modifier {
                                id: format!("captain_{}_{}", buff_stat_str(*stat), ch.id),
                                stat: buff_stat(*stat),
                                amount: *amount,
                                source: format!("captain_{cap_id}"),
                                duration: ModifierDuration::Permanent,
                                turns_remaining: None,
                            },
                        ));
                    }
                }
            }
        }
        for (id, m) in pushes {
            state.get_card_mut(&id)?.modifiers.push(m);
        }
    }

    // === Character passive buffs (buffAlly on other characters) ===
    for ch in &board_chars {
        let def = registry.get_card_def(&ch.def_id)?;
        let Some(passive) = &def.passive else {
            continue;
        };
        for effect in &passive.effects {
            if let PassiveEffect::BuffAlly {
                stat,
                amount,
                filter,
            } = effect
            {
                for target in &board_chars {
                    if target.id == ch.id {
                        continue; // Don't buff self
                    }
                    if matches_filter(registry, &target.def_id, filter.as_ref())? {
                        state.get_card_mut(&target.id)?.modifiers.push(Modifier {
                            id: format!("passive_{}_{}_{}", ch.id, buff_stat_str(*stat), target.id),
                            stat: buff_stat(*stat),
                            amount: *amount,
                            source: format!("passive_{}", ch.id),
                            duration: ModifierDuration::Permanent,
                            turns_remaining: None,
                        });
                    }
                }
            }
        }
    }

    // === Ship passive buffs ===
    if let Some(ship_id) = state.players.get(player_id).active_ship.clone() {
        if let Some(ship_card) = state.cards.get(&ship_id) {
            let ship_def = registry.get_card_def(&ship_card.def_id)?;
            if let Some(ship_passive) = &ship_def.ship_passive {
                let desc = ship_passive.to_lowercase();
                // Parse common ship passive patterns
                let ship_atk_bonus = ship_bonus(&desc, "atk");
                let ship_def_bonus = ship_bonus(&desc, "def");
                let ship_def_id = ship_def.id.clone();

                // Decision §8.10: one scope helper, shared with `deploy_cost` —
                // "Vos Baroque Works …" buffs Baroque Works units only.
                let scope = ship_passive_scope(&desc);

                for ch in &board_chars {
                    let char_def = registry.get_card_def(&ch.def_id)?;
                    if scope.matches(char_def) {
                        if ship_atk_bonus > 0 {
                            state.get_card_mut(&ch.id)?.modifiers.push(Modifier {
                                id: format!("ship_passive_atk_{}", ch.id),
                                stat: ModifierStat::Atk,
                                amount: ship_atk_bonus,
                                source: format!("passive_ship_{ship_def_id}"),
                                duration: ModifierDuration::Permanent,
                                turns_remaining: None,
                            });
                        }
                        if ship_def_bonus > 0 {
                            state.get_card_mut(&ch.id)?.modifiers.push(Modifier {
                                id: format!("ship_passive_def_{}", ch.id),
                                stat: ModifierStat::Def,
                                amount: ship_def_bonus,
                                source: format!("passive_ship_{ship_def_id}"),
                                duration: ModifierDuration::Permanent,
                                turns_remaining: None,
                            });
                        }
                    }
                }
            }
        }
    }

    // === Synergy bonuses ===
    // Decision §8.1-7: a synergy partner is a unit *on the board*; a recto
    // captain is off-board (spec §4.11), so the own captain only counts once it
    // is flipped into a slot (this is what makes `RH-004`/`CAP-SHANKS` work).
    let board_captain_def_id: Option<String> = {
        let cap = &state.players.get(player_id).captain;
        (cap.flipped && cap.slot.is_some()).then(|| cap.def_id.clone())
    };
    for ch in &board_chars {
        let def = registry.get_card_def(&ch.def_id)?;
        let Some(synergies) = &def.synergies else {
            continue;
        };
        for syn in synergies {
            // Check if partner is on the board
            let partner_on_board = board_chars.iter().any(|c| c.def_id == syn.partner_id)
                || board_captain_def_id.as_deref() == Some(syn.partner_id.as_str());
            if partner_on_board {
                state.get_card_mut(&ch.id)?.modifiers.push(Modifier {
                    id: format!("synergy_{}_{}", ch.id, syn.partner_id),
                    stat: ModifierStat::Atk,
                    amount: syn.atk_bonus,
                    source: format!("synergy_{}", syn.partner_id),
                    duration: ModifierDuration::Permanent,
                    turns_remaining: None,
                });
            }
        }
    }

    Ok(())
}

/// TS `applyEnemyDebuffAuras(state)` — recompute enemy-debuff auras (Shanks
/// "adjacent enemies -2 ATK", Goldenweek / Howling Gab "one enemy -N").
/// Approximations: adjacent → enemy front row; one → strongest enemy.
pub fn apply_enemy_debuff_auras(
    state: &mut GameState,
    registry: &CardRegistry,
) -> Result<(), EngineError> {
    let players = [PlayerId::Player1, PlayerId::Player2];
    for pid in players {
        for slot in Slot::ALL {
            if let Some(id) = state.players.get(pid).board.get(slot).cloned() {
                state
                    .get_card_mut(&id)?
                    .modifiers
                    .retain(|m| m.source != "debuffAura");
            }
        }
    }
    for pid in players {
        let opp = pid.opponent();
        let mut sources: Vec<Vec<PassiveEffect>> = Vec::new();
        for slot in Slot::ALL {
            if let Some(id) = state.players.get(pid).board.get(slot) {
                let def = registry.get_card_def(&state.get_card(id)?.def_id)?;
                sources.push(
                    def.passive
                        .as_ref()
                        .map(|p| p.effects.clone())
                        .unwrap_or_default(),
                );
            }
        }
        let cap = &state.players.get(pid).captain;
        let cd = registry.get_captain_def(&cap.def_id)?;
        sources.push(
            if cap.flipped {
                &cd.verso.passive
            } else {
                &cd.recto.passive
            }
            .effects
            .clone(),
        );

        let (mut adj, mut one) = (0, 0);
        for effs in &sources {
            for e in effs {
                match e {
                    PassiveEffect::DebuffAdjacentEnemies { amount } => adj += amount,
                    PassiveEffect::DebuffOneEnemy { amount } => one = one.max(*amount),
                    _ => {}
                }
            }
        }
        if adj > 0 {
            for s in Slot::FRONT {
                if let Some(id) = state.players.get(opp).board.get(s).cloned() {
                    state.get_card_mut(&id)?.modifiers.push(Modifier {
                        id: format!("debuffAura_adj_{id}"),
                        stat: ModifierStat::Atk,
                        amount: -adj,
                        source: "debuffAura".to_string(),
                        duration: ModifierDuration::Permanent,
                        turns_remaining: None,
                    });
                }
            }
        }
        if one > 0 {
            let mut best: Option<String> = None;
            let mut best_atk = -1;
            for slot in Slot::ALL {
                let Some(id) = state.players.get(opp).board.get(slot) else {
                    continue;
                };
                let a = registry
                    .get_card_def(&state.get_card(id)?.def_id)?
                    .atk
                    .unwrap_or(0);
                if a > best_atk {
                    best_atk = a;
                    best = Some(id.clone());
                }
            }
            if let Some(best) = best {
                state.get_card_mut(&best)?.modifiers.push(Modifier {
                    id: format!("debuffAura_one_{best}"),
                    stat: ModifierStat::Atk,
                    amount: -one,
                    source: "debuffAura".to_string(),
                    duration: ModifierDuration::Permanent,
                    turns_remaining: None,
                });
            }
        }
    }
    Ok(())
}

/// TS `matchesFilter(draft, defId, filter)` — no filter matches everything.
pub fn matches_filter(
    registry: &CardRegistry,
    def_id: &str,
    filter: Option<&AllyFilter>,
) -> Result<bool, EngineError> {
    let Some(filter) = filter else {
        return Ok(true);
    };
    let def = registry.get_card_def(def_id)?;
    if filter.faction.is_some_and(|f| def.faction != f) {
        return Ok(false);
    }
    // TS `if (filter.tag && ...)` — JS truthiness, so an empty tag is skipped.
    if filter
        .tag
        .as_ref()
        .is_some_and(|tag| !tag.is_empty() && !def.has_tag(tag))
    {
        return Ok(false);
    }
    if filter.trait_.is_some_and(|t| !def.has_trait(t)) {
        return Ok(false);
    }
    Ok(true)
}

// ============================================================
// On-KO effects
// ============================================================

/// TS `applyOnKOEffects(state, koPlayerId, killerPlayerId, koDefId)` —
/// everything that triggers when one of `ko_player_id`'s characters is KO'd.
pub fn apply_on_ko_effects(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &EngineContext,
    ko_player_id: PlayerId,
    killer_player_id: PlayerId,
    ko_def_id: &str,
) -> Result<(), EngineError> {
    // Track game/turn KO flags (free captain flip, Flashback condition).
    state.players.get_mut(ko_player_id).char_ko_ed_this_game = Some(true);

    // Captain onAllyKO passive (captain def / side read from the entry state)
    let (cap_def_name, cap_effects) = {
        let captain = &state.players.get(ko_player_id).captain;
        let cap_def = registry.get_captain_def(&captain.def_id)?;
        let cap_passive = if captain.flipped {
            &cap_def.verso.passive
        } else {
            &cap_def.recto.passive
        };
        (cap_def.name.clone(), cap_passive.effects.clone())
    };

    for effect in &cap_effects {
        if let PassiveEffect::OnAllyKo {
            effect: OnAllyKoEffectKind::BonusWill,
            amount,
        } = effect
        {
            state.players.get_mut(ko_player_id).volonte += amount;
            state.add_log(
                ko_player_id,
                format!("Passif Capitaine : +{amount} Vol. (allie KO)"),
            );
        }
        // Luffy verso: +1 ATK permanent per Mugiwara ally KO (max +3).
        if let PassiveEffect::SelfBuffOnAllyKo {
            stat,
            amount,
            max,
            filter,
        } = effect
        {
            if matches_filter(registry, ko_def_id, filter.as_ref())? {
                let cap = &mut state.players.get_mut(ko_player_id).captain;
                let current: i32 = cap
                    .modifiers
                    .iter()
                    .filter(|m| m.source == "captainSelfKO")
                    .map(|m| m.amount)
                    .sum();
                // Decision §8.1-4: `max` caps the accumulated total, so the
                // push is clamped to the room left instead of being allowed to
                // overshoot it (`amount` is 1 on every shipped captain).
                let gain = (*amount).min(*max - current);
                if gain > 0 {
                    cap.modifiers.push(Modifier {
                        id: format!("captainSelfKO_{}", ctx.now()),
                        stat: atk_stat(*stat),
                        amount: gain,
                        source: "captainSelfKO".to_string(),
                        duration: ModifierDuration::Permanent,
                        turns_remaining: None,
                    });
                    state.add_log(
                        ko_player_id,
                        format!("{cap_def_name} : +{gain} ATK permanent (Mugiwara KO)."),
                    );
                }
            }
        }
    }

    // Banish-on-KO (Akainu Justice Implacable): the killer's flipped captain banishes the victim.
    {
        let killer = &state.players.get(killer_player_id).captain;
        let killer_def = registry.get_captain_def(&killer.def_id)?;
        let killer_passive = if killer.flipped {
            &killer_def.verso.passive
        } else {
            &killer_def.recto.passive
        };
        if killer.flipped
            && killer_passive
                .effects
                .iter()
                .any(|e| matches!(e, PassiveEffect::BanishOnKo))
        {
            let gy: Vec<String> = state.players.get(ko_player_id).graveyard.clone();
            for i in (0..gy.len()).rev() {
                if state.get_card(&gy[i])?.def_id == ko_def_id {
                    state.get_card_mut(&gy[i])?.zone = Zone::Banished;
                    state.players.get_mut(ko_player_id).graveyard.remove(i);
                    break;
                }
            }
            let name = registry.get_card_def(ko_def_id)?.name.clone();
            state.add_log(
                killer_player_id,
                format!("{name} est banni (Justice Implacable) !"),
            );
        }
    }

    // Explode-on-KO (Mr. 5): the KO'd unit deals damage to an enemy.
    let ko_def_static = registry.get_card_def(ko_def_id)?;
    let explode: i32 = ko_def_static
        .passive
        .as_ref()
        .map(|p| {
            p.effects
                .iter()
                .map(|e| match e {
                    PassiveEffect::ExplodeOnKo { amount } => *amount,
                    _ => 0,
                })
                .sum()
        })
        .unwrap_or(0);
    if explode > 0 {
        let enemies = get_board_characters(state, killer_player_id);
        if !enemies.is_empty() {
            // `enemies.reduce((a, b) => (b.currentPv < a.currentPv ? b : a))` — first minimum
            let mut target = enemies[0];
            for b in &enemies[1..] {
                if b.current_pv < target.current_pv {
                    target = b;
                }
            }
            let tid = target.instance_id.clone();
            let target_name = registry.get_card_def(&target.def_id)?.name.clone();
            state.get_card_mut(&tid)?.current_pv -= explode;
            state.add_log(
                ko_player_id,
                format!("Corps Explosif : {explode} dégâts à {target_name} !"),
            );
            if let Some(c) = state.cards.get(&tid) {
                if c.current_pv <= 0 {
                    let ex_def = c.def_id.clone();
                    let ex_owner = c.owner;
                    let ex_name = registry.get_card_def(&ex_def)?.name.clone();
                    state.add_log(ex_owner, format!("{ex_name} est KO !"));
                    remove_from_board(state, registry, &tid)?;
                }
            }
        }
    }

    // Synergy rage bonus (partner KO'd → surviving partner gets temp ATK)
    let board_chars: Vec<(String, String)> = get_board_characters(state, ko_player_id)
        .into_iter()
        .map(|c| (c.instance_id.clone(), c.def_id.clone()))
        .collect();
    for (char_id, char_def_id) in board_chars {
        let def = registry.get_card_def(&char_def_id)?;
        let Some(synergies) = &def.synergies else {
            continue;
        };
        for syn in synergies {
            // TS `syn.onPartnerKO` truthiness: absent or 0 does not trigger.
            let Some(rage) = syn.on_partner_ko.filter(|n| *n != 0) else {
                continue;
            };
            if syn.partner_id == ko_def_id {
                let ko_name = registry.get_card_def(ko_def_id)?.name.clone();
                state.get_card_mut(&char_id)?.modifiers.push(Modifier {
                    id: format!("synrage_{char_id}_{}", ctx.now()),
                    stat: ModifierStat::Atk,
                    amount: rage,
                    // Decision §8.1-1: `synrage_` (not `synergy_`) so the
                    // trailing `recalculate_passive_buffs` — which rebuilds the
                    // *continuous* `synergy_` buffs — cannot wipe the rage buff
                    // it was just given. It expires in `reset_turn_flags`.
                    source: format!("synrage_{ko_def_id}"),
                    duration: ModifierDuration::Turn,
                    turns_remaining: None,
                });
                state.add_log(
                    ko_player_id,
                    format!("{} : rage ! +{rage} ATK ({ko_name} KO)", def.name),
                );
            }
        }
    }

    // Recalculate passive buffs (someone left the board)
    recalculate_passive_buffs(state, registry, ko_player_id)?;

    Ok(())
}

// ============================================================
// Tests — the trickiest branches of passives.ts
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Board, CaptainInstance, CardInstance, PlayerState, Players};
    use crate::types::{
        BaseAction, CaptainDef, CaptainRecto, CaptainVerso, CardDef, CardType, EntryEffect,
        Faction, FlipCondition, PassiveDef, Phase, Rarity, SpecialAttack, SynergyDef, Trait,
    };
    use std::collections::BTreeMap;

    // --- builders -------------------------------------------------------

    fn chr(id: &str, name: &str, atk: i32, def: i32, pv: i32) -> CardDef {
        let mut d = CardDef::new(
            id,
            name,
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

    fn ship(id: &str, name: &str, passive: &str) -> CardDef {
        let mut d = CardDef::new(
            id,
            name,
            CardType::Ship,
            2,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        d.ship_passive = Some(passive.to_string());
        d
    }

    fn passive(effects: Vec<PassiveEffect>) -> PassiveDef {
        PassiveDef {
            name: "P".to_string(),
            description: "D".to_string(),
            effects,
        }
    }

    fn cap(
        id: &str,
        name: &str,
        recto: Vec<PassiveEffect>,
        verso: Vec<PassiveEffect>,
    ) -> CaptainDef {
        CaptainDef {
            id: id.to_string(),
            name: name.to_string(),
            faction: Faction::Pirate,
            tags: None,
            traits: None,
            recto: CaptainRecto {
                pv: 20,
                atk: 3,
                def: 2,
                passive: passive(recto),
                attacks: Vec::new(),
                surcharge: None,
            },
            flip_condition: FlipCondition::default(),
            verso: CaptainVerso {
                pv: 20,
                atk: 4,
                def: 3,
                passive: passive(verso),
                entry_effect: EntryEffect::GrantSelfRush,
                base_action: BaseAction::default(),
                special_attack: SpecialAttack::default(),
                surcharge: None,
                traits: None,
                natural_haki: None,
            },
        }
    }

    fn player(id: PlayerId, cap_def_id: &str) -> PlayerState {
        PlayerState {
            id,
            captain: CaptainInstance::new(cap_def_id.to_string(), id, 20),
            deck: Vec::new(),
            hand: Vec::new(),
            graveyard: Vec::new(),
            board: Board::empty(),
            active_ship: None,
            volonte: 0,
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
                player1: player(PlayerId::Player1, cap1),
                player2: player(PlayerId::Player2, cap2),
            },
            turn_number: 3,
            current_player: PlayerId::Player1,
            phase: Phase::Main,
            pending_attack: None,
            log: Vec::new(),
            winner: None,
            first_player: PlayerId::Player1,
        }
    }

    /// Put `def_id` on `owner`'s `slot` with `pv` current PV; returns the instance id.
    fn place(
        state: &mut GameState,
        registry: &CardRegistry,
        owner: PlayerId,
        slot: Slot,
        def_id: &str,
        pv: i32,
    ) -> String {
        let _ = registry;
        let iid = format!("{def_id}@{}{}", owner.as_str(), slot.as_str());
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

    fn atk_bonus(state: &GameState, iid: &str) -> i32 {
        state
            .cards
            .get(iid)
            .unwrap()
            .modifiers
            .iter()
            .filter(|m| m.stat == ModifierStat::Atk)
            .map(|m| m.amount)
            .sum()
    }

    fn last_msg(state: &GameState) -> &str {
        &state.log.last().unwrap().message
    }

    // --- ship_bonus (the TS `desc.match(/\+(\d+)\s*atk/i)` port) ---------

    #[test]
    fn ship_bonus_matches_the_real_card_texts() {
        // The four distinct shipPassive strings in src/data/cards, lower-cased.
        let baroque = "vos baroque works gagnent +1 atk.";
        let marine_pv = "vos marine gagnent +1 pv.";
        let mugi_pv = "vos mugiwara ont +1 pv au déploiement.";
        let redhair = "vos personnages gagnent +1 atk.";
        assert_eq!(ship_bonus(baroque, "atk"), 1);
        assert_eq!(ship_bonus(baroque, "def"), 0);
        assert_eq!(ship_bonus(marine_pv, "atk"), 0);
        assert_eq!(ship_bonus(mugi_pv, "atk"), 0);
        assert_eq!(ship_bonus(redhair, "atk"), 1);
        // Multi-bonus text: each keyword picks its own `+N`, like the two regexes.
        assert_eq!(ship_bonus("+2 atk et +3 def", "atk"), 2);
        assert_eq!(ship_bonus("+2 atk et +3 def", "def"), 3);
        // No whitespace, multi-digit, and a `+` with no digits behind it.
        assert_eq!(ship_bonus("++12atk", "atk"), 12);
        // `\+(\d+)\s*def` does not match "+12 3def" (JS backtracking fails too).
        assert_eq!(ship_bonus("+12 3def", "def"), 0);
        // `parseInt` is a double: a digit run wider than `i32` still yields a
        // large positive bonus that passes the `> 0` guard, never 0.
        assert_eq!(ship_bonus("+99999999999 atk", "atk"), i32::MAX);
        // JS `\s` includes U+FEFF but not U+0085.
        assert_eq!(ship_bonus("+1\u{feff}atk", "atk"), 1);
        assert_eq!(ship_bonus("+1\u{85}atk", "atk"), 0);
    }

    // --- matchesFilter ---------------------------------------------------

    #[test]
    fn matches_filter_checks_faction_tag_and_trait() {
        let mut d = chr("C1", "C1", 2, 1, 3);
        d.tags = Some(vec!["mugiwara".to_string()]);
        d.traits = Some(vec![Trait::Shield]);
        let reg = CardRegistry::from_sets([vec![d]], []);

        assert!(matches_filter(&reg, "C1", None).unwrap());
        assert!(
            matches_filter(
                &reg,
                "C1",
                Some(&AllyFilter {
                    faction: Some(Faction::Pirate),
                    ..Default::default()
                })
            )
            .unwrap()
        );
        assert!(
            !matches_filter(
                &reg,
                "C1",
                Some(&AllyFilter {
                    faction: Some(Faction::Marine),
                    ..Default::default()
                })
            )
            .unwrap()
        );
        assert!(
            matches_filter(
                &reg,
                "C1",
                Some(&AllyFilter {
                    tag: Some("mugiwara".to_string()),
                    ..Default::default()
                })
            )
            .unwrap()
        );
        assert!(
            !matches_filter(
                &reg,
                "C1",
                Some(&AllyFilter {
                    tag: Some("marine".to_string()),
                    ..Default::default()
                })
            )
            .unwrap()
        );
        assert!(
            matches_filter(
                &reg,
                "C1",
                Some(&AllyFilter {
                    trait_: Some(Trait::Shield),
                    ..Default::default()
                })
            )
            .unwrap()
        );
        // TS `if (filter.tag && …)` — an empty tag is falsy, so the tag check
        // is skipped entirely and the filter still matches.
        assert!(
            matches_filter(
                &reg,
                "C1",
                Some(&AllyFilter {
                    tag: Some(String::new()),
                    ..Default::default()
                })
            )
            .unwrap()
        );
    }

    // --- start-of-turn passives -----------------------------------------

    #[test]
    fn heal_adjacent_caps_at_def_pv_and_logs_verbatim() {
        // Healer in V2 → adjacents are V1, V3, A2 (ADJACENCY order).
        let healer = {
            let mut d = chr("H", "Docteur", 1, 1, 3);
            d.passive = Some(passive(vec![PassiveEffect::HealAdjacent { amount: 1 }]));
            d
        };
        let reg = CardRegistry::from_sets(
            [vec![
                healer,
                chr("A", "Ally", 2, 1, 4),
                chr("B", "Far", 2, 1, 4),
            ]],
            [cap("CAP", "Cap", vec![], vec![])],
        );
        let mut st = blank_state("CAP", "CAP");
        place(&mut st, &reg, PlayerId::Player1, Slot::V2, "H", 3);
        let a = place(&mut st, &reg, PlayerId::Player1, Slot::V1, "A", 1); // heals to 2
        let b = place(&mut st, &reg, PlayerId::Player1, Slot::A2, "A", 4); // already at max
        let far = place(&mut st, &reg, PlayerId::Player1, Slot::A1, "B", 1); // not adjacent

        apply_start_of_turn_passives(&mut st, &reg, &EngineContext::seeded(1), PlayerId::Player1)
            .unwrap();

        assert_eq!(st.cards[&a].current_pv, 2);
        assert_eq!(st.cards[&b].current_pv, 4, "capped at def.pv");
        assert_eq!(st.cards[&far].current_pv, 1, "A1 is not adjacent to V2");
        assert_eq!(last_msg(&st), "Docteur soigne 1 PV aux adjacents");
    }

    #[test]
    fn heal_adjacent_logs_even_with_no_adjacent_ally() {
        let healer = {
            let mut d = chr("H", "Docteur", 1, 1, 3);
            d.passive = Some(passive(vec![PassiveEffect::HealAdjacent { amount: 2 }]));
            d
        };
        let reg = CardRegistry::from_sets([vec![healer]], [cap("CAP", "Cap", vec![], vec![])]);
        let mut st = blank_state("CAP", "CAP");
        place(&mut st, &reg, PlayerId::Player1, Slot::V2, "H", 3);

        apply_start_of_turn_passives(&mut st, &reg, &EngineContext::seeded(1), PlayerId::Player1)
            .unwrap();

        assert_eq!(st.log.len(), 1);
        assert_eq!(last_msg(&st), "Docteur soigne 2 PV aux adjacents");
    }

    #[test]
    fn start_turn_buff_ally_picks_first_highest_atk_non_source() {
        let usopp = {
            let mut d = chr("U", "Usopp", 1, 1, 3);
            d.passive = Some(passive(vec![PassiveEffect::StartTurnBuffAlly {
                stat: AtkDefStat::Atk,
                amount: 1,
            }]));
            d
        };
        // Two allies tie on ATK=4 → strict `>` keeps the first in slot order (V1).
        let reg = CardRegistry::from_sets(
            [vec![
                usopp,
                chr("S", "Strong", 4, 1, 4),
                chr("W", "Weak", 2, 1, 4),
            ]],
            [cap("CAP", "Cap", vec![], vec![])],
        );
        let mut st = blank_state("CAP", "CAP");
        place(&mut st, &reg, PlayerId::Player1, Slot::V2, "U", 3);
        let first = place(&mut st, &reg, PlayerId::Player1, Slot::V1, "S", 4);
        let tie = place(&mut st, &reg, PlayerId::Player1, Slot::V3, "S", 4);
        let weak = place(&mut st, &reg, PlayerId::Player1, Slot::A1, "W", 4);

        let ctx = EngineContext::new(1, 7);
        apply_start_of_turn_passives(&mut st, &reg, &ctx, PlayerId::Player1).unwrap();

        assert_eq!(atk_bonus(&st, &first), 1);
        assert_eq!(atk_bonus(&st, &tie), 0);
        assert_eq!(atk_bonus(&st, &weak), 0);
        let m = &st.cards[&first].modifiers[0];
        assert_eq!(m.id, format!("vantardise_{first}_7"));
        // Decision §8.1-2 (was `passive_U@player1V2`, which the passive strip
        // removed on the next deploy/equip/KO/flip).
        assert_eq!(m.source, "vantardise_U@player1V2");
        assert_eq!(m.duration, ModifierDuration::Turn);
        assert_eq!(
            last_msg(&st),
            "Usopp : Vantardise — un allié gagne +1 ATK ce tour."
        );

        // It survives a later recalculation…
        recalculate_passive_buffs(&mut st, &reg, PlayerId::Player1).unwrap();
        assert_eq!(atk_bonus(&st, &first), 1);
        assert!(
            st.cards[&first]
                .modifiers
                .iter()
                .any(|m| m.source == "vantardise_U@player1V2")
        );
        // …and expires with the turn.
        st.reset_turn_flags();
        assert_eq!(atk_bonus(&st, &first), 0);
    }

    #[test]
    fn start_turn_buff_ally_is_a_no_op_when_alone() {
        let usopp = {
            let mut d = chr("U", "Usopp", 1, 1, 3);
            d.passive = Some(passive(vec![PassiveEffect::StartTurnBuffAlly {
                stat: AtkDefStat::Def,
                amount: 2,
            }]));
            d
        };
        let reg = CardRegistry::from_sets([vec![usopp]], [cap("CAP", "Cap", vec![], vec![])]);
        let mut st = blank_state("CAP", "CAP");
        let u = place(&mut st, &reg, PlayerId::Player1, Slot::V2, "U", 3);

        apply_start_of_turn_passives(&mut st, &reg, &EngineContext::seeded(1), PlayerId::Player1)
            .unwrap();

        assert!(st.log.is_empty(), "no ally → early return, no log");
        assert!(st.cards[&u].modifiers.is_empty());
    }

    // --- recalculatePassiveBuffs ----------------------------------------

    fn recalc_registry() -> CardRegistry {
        let mut leader = chr("L", "Leader", 2, 1, 4);
        leader.tags = Some(vec!["mugiwara".to_string()]);
        leader.passive = Some(passive(vec![PassiveEffect::BuffAlly {
            stat: BuffStat::Def,
            amount: 1,
            filter: None,
        }]));
        let mut mate = chr("M", "Mate", 3, 1, 4);
        mate.tags = Some(vec!["mugiwara".to_string()]);
        mate.synergies = Some(vec![SynergyDef {
            partner_id: "L".to_string(),
            atk_bonus: 2,
            on_partner_ko: Some(3),
        }]);
        let mut outsider = chr("O", "Outsider", 1, 1, 4);
        outsider.faction = Faction::Marine;
        CardRegistry::from_sets(
            [vec![
                leader,
                mate,
                outsider,
                ship("SH", "Ship", "Vos Mugiwara ont +1 ATK."),
                ship("SH2", "Ship2", "Vos personnages gagnent +1 ATK."),
            ]],
            [cap(
                "CAP",
                "Cap",
                vec![PassiveEffect::BuffAlly {
                    stat: BuffStat::Atk,
                    amount: 1,
                    filter: Some(AllyFilter {
                        faction: Some(Faction::Pirate),
                        ..Default::default()
                    }),
                }],
                vec![],
            )],
        )
    }

    #[test]
    fn recalculate_strips_only_passive_captain_synergy_sources() {
        let reg = recalc_registry();
        let mut st = blank_state("CAP", "CAP");
        let l = place(&mut st, &reg, PlayerId::Player1, Slot::V1, "L", 4);
        let keep = Modifier {
            id: "keepme".to_string(),
            stat: ModifierStat::Atk,
            amount: 5,
            source: "debuffAura".to_string(),
            duration: ModifierDuration::Permanent,
            turns_remaining: None,
        };
        let card = st.cards.get_mut(&l).unwrap();
        card.modifiers.push(keep.clone());
        for src in ["passive_X", "captain_Y", "synergy_Z", "passive_ship_S"] {
            card.modifiers.push(Modifier {
                id: src.to_string(),
                stat: ModifierStat::Atk,
                amount: 9,
                source: src.to_string(),
                duration: ModifierDuration::Permanent,
                turns_remaining: None,
            });
        }
        // `captainSelfKO` has no underscore after "captain" → survives.
        card.modifiers.push(Modifier {
            id: "self".to_string(),
            stat: ModifierStat::Atk,
            amount: 1,
            source: "captainSelfKO".to_string(),
            duration: ModifierDuration::Permanent,
            turns_remaining: None,
        });

        recalculate_passive_buffs(&mut st, &reg, PlayerId::Player1).unwrap();

        let sources: Vec<&str> = st.cards[&l]
            .modifiers
            .iter()
            .map(|m| m.source.as_str())
            .collect();
        assert_eq!(sources, vec!["debuffAura", "captainSelfKO", "captain_CAP"]);
    }

    #[test]
    fn recalculate_applies_captain_character_ship_and_synergy_buffs() {
        let reg = recalc_registry();
        let mut st = blank_state("CAP", "CAP");
        let l = place(&mut st, &reg, PlayerId::Player1, Slot::V1, "L", 4);
        let m = place(&mut st, &reg, PlayerId::Player1, Slot::V2, "M", 4);
        let o = place(&mut st, &reg, PlayerId::Player1, Slot::A1, "O", 4);
        // Active ship: "Vos Mugiwara ont +1 ATK." → only cards tagged mugiwara.
        let mut sh = CardInstance::new(
            "shipinst".to_string(),
            "SH".to_string(),
            PlayerId::Player1,
            0,
        );
        sh.zone = Zone::Board;
        st.cards.insert("shipinst".to_string(), sh);
        st.players.get_mut(PlayerId::Player1).active_ship = Some("shipinst".to_string());

        recalculate_passive_buffs(&mut st, &reg, PlayerId::Player1).unwrap();

        // Leader: captain +1 ATK (pirate), Mate's passive is none, ship +1 ATK (mugiwara tag).
        let l_srcs: Vec<(&str, i32)> = st.cards[&l]
            .modifiers
            .iter()
            .map(|x| (x.source.as_str(), x.amount))
            .collect();
        assert_eq!(l_srcs, vec![("captain_CAP", 1), ("passive_ship_SH", 1)]);
        // Mate: captain +1 ATK, Leader's buffAlly +1 DEF, ship +1 ATK, synergy(L) +2 ATK.
        let m_srcs: Vec<(&str, i32)> = st.cards[&m]
            .modifiers
            .iter()
            .map(|x| (x.source.as_str(), x.amount))
            .collect();
        assert_eq!(
            m_srcs,
            vec![
                ("captain_CAP", 1),
                ("passive_L@player1V1", 1),
                ("passive_ship_SH", 1),
                ("synergy_L", 2),
            ]
        );
        assert_eq!(
            st.cards[&m].modifiers[3].id,
            format!("synergy_{m}_L"),
            "synergy modifier id"
        );
        // Outsider: Marine → captain filter fails, no mugiwara tag → no ship buff.
        // Only the Leader's unfiltered buffAlly +1 DEF applies.
        let o_srcs: Vec<(&str, i32)> = st.cards[&o]
            .modifiers
            .iter()
            .map(|x| (x.source.as_str(), x.amount))
            .collect();
        assert_eq!(o_srcs, vec![("passive_L@player1V1", 1)]);
        // Leader never buffs itself.
        assert!(
            !st.cards[&l]
                .modifiers
                .iter()
                .any(|x| x.source == "passive_L@player1V1")
        );
    }

    #[test]
    fn ship_without_faction_word_buffs_everyone() {
        let reg = recalc_registry();
        let mut st = blank_state("CAP", "CAP");
        let o = place(&mut st, &reg, PlayerId::Player1, Slot::V1, "O", 4);
        let mut sh = CardInstance::new("s2".to_string(), "SH2".to_string(), PlayerId::Player1, 0);
        sh.zone = Zone::Board;
        st.cards.insert("s2".to_string(), sh);
        st.players.get_mut(PlayerId::Player1).active_ship = Some("s2".to_string());

        recalculate_passive_buffs(&mut st, &reg, PlayerId::Player1).unwrap();

        assert_eq!(atk_bonus(&st, &o), 1, "no mugiwara/marine word → all match");
    }

    // --- applyEnemyDebuffAuras ------------------------------------------

    #[test]
    fn debuff_auras_hit_front_row_and_strongest_by_base_atk() {
        let mut goldenweek = chr("G", "Goldenweek", 1, 1, 3);
        goldenweek.passive = Some(passive(vec![PassiveEffect::DebuffOneEnemy { amount: 2 }]));
        let mut gab = chr("GB", "Gab", 1, 1, 3);
        gab.passive = Some(passive(vec![PassiveEffect::DebuffOneEnemy { amount: 1 }]));
        let reg = CardRegistry::from_sets(
            [vec![
                goldenweek,
                gab,
                chr("BIG", "Big", 5, 1, 5),
                chr("SM", "Small", 1, 1, 5),
            ]],
            [
                cap("CAP", "Cap", vec![], vec![]),
                cap(
                    "SHANKS",
                    "Shanks",
                    vec![PassiveEffect::DebuffAdjacentEnemies { amount: 2 }],
                    vec![],
                ),
            ],
        );
        let mut st = blank_state("SHANKS", "CAP");
        // player1 (Shanks) debuffs player2's whole front row by 2.
        // player2 fields Goldenweek + Gab → `one = max(2, 1) = 2` on player1's strongest.
        place(&mut st, &reg, PlayerId::Player2, Slot::V1, "G", 3);
        place(&mut st, &reg, PlayerId::Player2, Slot::A1, "GB", 3);
        let e_front = place(&mut st, &reg, PlayerId::Player2, Slot::V2, "SM", 5);
        let big = place(&mut st, &reg, PlayerId::Player1, Slot::A3, "BIG", 5);
        let small = place(&mut st, &reg, PlayerId::Player1, Slot::V1, "SM", 5);
        // BIG sits in the back row: `one` targets it anyway (highest base ATK).
        st.cards.get_mut(&small).unwrap().modifiers.push(Modifier {
            id: "stale".to_string(),
            stat: ModifierStat::Atk,
            amount: -99,
            source: "debuffAura".to_string(),
            duration: ModifierDuration::Permanent,
            turns_remaining: None,
        });

        apply_enemy_debuff_auras(&mut st, &reg).unwrap();

        assert_eq!(
            atk_bonus(&st, &big),
            -2,
            "one → strongest, stale aura cleared"
        );
        assert_eq!(atk_bonus(&st, &small), 0, "stale debuffAura removed");
        assert_eq!(atk_bonus(&st, &e_front), -2, "adjacent → enemy front row");
        // The Goldenweek in V1 is also front row for player1's Shanks aura.
        let g = st
            .players
            .get(PlayerId::Player2)
            .board
            .get(Slot::V1)
            .unwrap()
            .clone();
        assert_eq!(atk_bonus(&st, &g), -2);
        // A1 is back row → untouched by the "adjacent" aura.
        let gb = st
            .players
            .get(PlayerId::Player2)
            .board
            .get(Slot::A1)
            .unwrap()
            .clone();
        assert_eq!(atk_bonus(&st, &gb), 0);
    }

    #[test]
    fn debuff_adjacent_amounts_stack_but_one_takes_the_max() {
        let mut src = chr("D", "Debuffer", 1, 1, 3);
        src.passive = Some(passive(vec![
            PassiveEffect::DebuffAdjacentEnemies { amount: 1 },
            PassiveEffect::DebuffOneEnemy { amount: 1 },
        ]));
        let reg = CardRegistry::from_sets(
            [vec![src, chr("T", "T", 2, 1, 4)]],
            [
                cap(
                    "CAPD",
                    "CapD",
                    vec![
                        PassiveEffect::DebuffAdjacentEnemies { amount: 2 },
                        PassiveEffect::DebuffOneEnemy { amount: 3 },
                    ],
                    vec![],
                ),
                cap("CAP", "Cap", vec![], vec![]),
            ],
        );
        let mut st = blank_state("CAPD", "CAP");
        place(&mut st, &reg, PlayerId::Player1, Slot::A1, "D", 3);
        let t = place(&mut st, &reg, PlayerId::Player2, Slot::V1, "T", 4);

        apply_enemy_debuff_auras(&mut st, &reg).unwrap();

        // adj = 1 + 2 (summed), one = max(1, 3) = 3 → both land on the lone enemy.
        let amounts: Vec<i32> = st.cards[&t].modifiers.iter().map(|m| m.amount).collect();
        assert_eq!(amounts, vec![-3, -3]);
        assert_eq!(st.cards[&t].modifiers[0].id, format!("debuffAura_adj_{t}"));
        assert_eq!(st.cards[&t].modifiers[1].id, format!("debuffAura_one_{t}"));
    }

    // --- applyOnKOEffects ------------------------------------------------

    #[test]
    fn on_ko_grants_bonus_will_and_sets_the_game_flag() {
        let reg = CardRegistry::from_sets(
            [vec![chr("V", "Victim", 1, 1, 1)]],
            [cap(
                "CAP",
                "Cap",
                vec![PassiveEffect::OnAllyKo {
                    effect: OnAllyKoEffectKind::BonusWill,
                    amount: 2,
                }],
                vec![],
            )],
        );
        let mut st = blank_state("CAP", "CAP");
        st.players.get_mut(PlayerId::Player1).volonte = 1;

        apply_on_ko_effects(
            &mut st,
            &reg,
            &EngineContext::seeded(1),
            PlayerId::Player1,
            PlayerId::Player2,
            "V",
        )
        .unwrap();

        assert_eq!(st.players.get(PlayerId::Player1).volonte, 3);
        assert_eq!(
            st.players.get(PlayerId::Player1).char_ko_ed_this_game,
            Some(true)
        );
        assert_eq!(st.log[0].message, "Passif Capitaine : +2 Vol. (allie KO)");
        assert_eq!(st.log[0].player, PlayerId::Player1);
    }

    #[test]
    fn self_buff_on_ally_ko_stops_at_max_and_respects_the_filter() {
        let mut victim = chr("V", "Victim", 1, 1, 1);
        victim.tags = Some(vec!["mugiwara".to_string()]);
        let reg = CardRegistry::from_sets(
            [vec![victim, chr("X", "Stranger", 1, 1, 1)]],
            [cap(
                "CAP",
                "Luffy",
                vec![PassiveEffect::SelfBuffOnAllyKo {
                    stat: AtkStat::Atk,
                    amount: 1,
                    max: 3,
                    filter: Some(AllyFilter {
                        tag: Some("mugiwara".to_string()),
                        ..Default::default()
                    }),
                }],
                vec![],
            )],
        );
        let ctx = EngineContext::new(1, 42);
        let mut st = blank_state("CAP", "CAP");

        // Non-Mugiwara KO → filter fails, nothing happens.
        apply_on_ko_effects(
            &mut st,
            &reg,
            &ctx,
            PlayerId::Player1,
            PlayerId::Player2,
            "X",
        )
        .unwrap();
        assert!(
            st.players
                .get(PlayerId::Player1)
                .captain
                .modifiers
                .is_empty()
        );

        for _ in 0..5 {
            apply_on_ko_effects(
                &mut st,
                &reg,
                &ctx,
                PlayerId::Player1,
                PlayerId::Player2,
                "V",
            )
            .unwrap();
        }
        let mods = &st.players.get(PlayerId::Player1).captain.modifiers;
        assert_eq!(mods.len(), 3, "current < max is checked before each push");
        assert_eq!(mods[0].id, "captainSelfKO_42");
        assert_eq!(mods[0].source, "captainSelfKO");
        assert_eq!(mods[0].duration, ModifierDuration::Permanent);
        assert_eq!(
            st.log.last().unwrap().message,
            "Luffy : +1 ATK permanent (Mugiwara KO)."
        );
    }

    #[test]
    fn banish_on_ko_removes_the_last_matching_graveyard_copy_and_always_logs() {
        let reg = CardRegistry::from_sets(
            [vec![chr("V", "Victime", 1, 1, 1)]],
            [
                cap("CAP", "Cap", vec![], vec![]),
                cap("AKAINU", "Akainu", vec![], vec![PassiveEffect::BanishOnKo]),
            ],
        );
        let mut st = blank_state("CAP", "AKAINU");
        st.players.get_mut(PlayerId::Player2).captain.flipped = true;
        for n in 0..2 {
            let iid = format!("v{n}");
            let mut inst = CardInstance::new(iid.clone(), "V".to_string(), PlayerId::Player1, 0);
            inst.zone = Zone::Graveyard;
            st.cards.insert(iid.clone(), inst);
            st.players.get_mut(PlayerId::Player1).graveyard.push(iid);
        }

        apply_on_ko_effects(
            &mut st,
            &reg,
            &EngineContext::seeded(1),
            PlayerId::Player1,
            PlayerId::Player2,
            "V",
        )
        .unwrap();

        // Reverse scan → the *last* graveyard entry is banished.
        assert_eq!(st.cards["v1"].zone, Zone::Banished);
        assert_eq!(st.cards["v0"].zone, Zone::Graveyard);
        assert_eq!(st.players.get(PlayerId::Player1).graveyard, vec!["v0"]);
        let entry = st.log.last().unwrap();
        assert_eq!(entry.message, "Victime est banni (Justice Implacable) !");
        assert_eq!(entry.player, PlayerId::Player2, "logged for the killer");
    }

    #[test]
    fn banish_on_ko_needs_a_flipped_killer_captain() {
        let reg = CardRegistry::from_sets(
            [vec![chr("V", "Victime", 1, 1, 1)]],
            [
                cap("CAP", "Cap", vec![], vec![]),
                cap("AKAINU", "Akainu", vec![], vec![PassiveEffect::BanishOnKo]),
            ],
        );
        let mut st = blank_state("CAP", "AKAINU"); // not flipped → recto passive is empty
        let mut inst = CardInstance::new("v0".to_string(), "V".to_string(), PlayerId::Player1, 0);
        inst.zone = Zone::Graveyard;
        st.cards.insert("v0".to_string(), inst);
        st.players
            .get_mut(PlayerId::Player1)
            .graveyard
            .push("v0".to_string());

        apply_on_ko_effects(
            &mut st,
            &reg,
            &EngineContext::seeded(1),
            PlayerId::Player1,
            PlayerId::Player2,
            "V",
        )
        .unwrap();

        assert_eq!(st.cards["v0"].zone, Zone::Graveyard);
        assert!(st.log.is_empty());
    }

    #[test]
    fn explode_on_ko_hits_the_first_lowest_pv_enemy_and_can_ko_it() {
        let mut bomb = chr("B", "Mr. 5", 1, 1, 1);
        bomb.passive = Some(passive(vec![PassiveEffect::ExplodeOnKo { amount: 2 }]));
        let reg = CardRegistry::from_sets(
            [vec![bomb, chr("E", "Ennemi", 2, 1, 4)]],
            [cap("CAP", "Cap", vec![], vec![])],
        );
        let mut st = blank_state("CAP", "CAP");
        // Two enemies tied at 2 PV → `reduce` with strict `<` keeps the first (V1).
        let first = place(&mut st, &reg, PlayerId::Player2, Slot::V1, "E", 2);
        let tie = place(&mut st, &reg, PlayerId::Player2, Slot::V2, "E", 2);
        let fat = place(&mut st, &reg, PlayerId::Player2, Slot::V3, "E", 4);

        apply_on_ko_effects(
            &mut st,
            &reg,
            &EngineContext::seeded(1),
            PlayerId::Player1,
            PlayerId::Player2,
            "B",
        )
        .unwrap();

        assert_eq!(st.cards[&first].current_pv, 0);
        assert_eq!(st.cards[&tie].current_pv, 2);
        assert_eq!(st.cards[&fat].current_pv, 4);
        assert_eq!(st.cards[&first].zone, Zone::Graveyard, "0 PV → removed");
        assert!(
            st.players
                .get(PlayerId::Player2)
                .board
                .get(Slot::V1)
                .is_none()
        );
        let msgs: Vec<&str> = st.log.iter().map(|l| l.message.as_str()).collect();
        assert_eq!(
            msgs,
            vec!["Corps Explosif : 2 dégâts à Ennemi !", "Ennemi est KO !",]
        );
        assert_eq!(
            st.log[0].player,
            PlayerId::Player1,
            "logged for the KO'd side"
        );
        assert_eq!(st.log[1].player, PlayerId::Player2, "logged for the owner");
    }

    #[test]
    fn explode_on_ko_leaves_a_survivor_on_the_board() {
        let mut bomb = chr("B", "Mr. 5", 1, 1, 1);
        bomb.passive = Some(passive(vec![PassiveEffect::ExplodeOnKo { amount: 2 }]));
        let reg = CardRegistry::from_sets(
            [vec![bomb, chr("E", "Ennemi", 2, 1, 4)]],
            [cap("CAP", "Cap", vec![], vec![])],
        );
        let mut st = blank_state("CAP", "CAP");
        let e = place(&mut st, &reg, PlayerId::Player2, Slot::V1, "E", 4);

        apply_on_ko_effects(
            &mut st,
            &reg,
            &EngineContext::seeded(1),
            PlayerId::Player1,
            PlayerId::Player2,
            "B",
        )
        .unwrap();

        assert_eq!(st.cards[&e].current_pv, 2);
        assert_eq!(st.cards[&e].zone, Zone::Board);
        assert_eq!(st.log.len(), 1);
    }

    #[test]
    fn synergy_rage_buffs_the_surviving_partner_this_turn() {
        let reg = recalc_registry();
        let mut st = blank_state("CAP", "CAP");
        let m = place(&mut st, &reg, PlayerId::Player1, Slot::V2, "M", 4);
        let ctx = EngineContext::new(1, 99);

        apply_on_ko_effects(
            &mut st,
            &reg,
            &ctx,
            PlayerId::Player1,
            PlayerId::Player2,
            "L",
        )
        .unwrap();

        assert_eq!(
            st.log.last().unwrap().message,
            "Mate : rage ! +3 ATK (Leader KO)"
        );
        // Decision §8.1-1: the rage modifier is pushed with `synrage_<koDefId>`,
        // which the trailing `recalculatePassiveBuffs` no longer strips (it used
        // to be `synergy_rage_L`, wiped on the very same call).
        let rage = st.cards[&m]
            .modifiers
            .iter()
            .find(|x| x.source == "synrage_L")
            .expect("the rage buff survives the trailing recalculation");
        assert_eq!(rage.amount, 3);
        assert_eq!(rage.duration, ModifierDuration::Turn);
        // The partner is gone from the board, so no continuous `synergy_L` buff:
        // captain buffAlly (+1) + rage (+3).
        assert_eq!(atk_bonus(&st, &m), 4);

        // …and it lasts exactly one turn.
        st.reset_turn_flags();
        assert_eq!(atk_bonus(&st, &m), 1, "only the captain buffAlly remains");
    }

    #[test]
    fn self_buff_on_ally_ko_clamps_the_last_push_to_the_max() {
        // Decision §8.1-4: `max` caps the accumulated total. With `amount = 2`
        // and `max = 3` the second KO may only add 1, and a third adds nothing.
        let reg = CardRegistry::from_sets(
            [vec![chr("V", "Victim", 1, 1, 1)]],
            [cap(
                "CAP",
                "Luffy",
                vec![PassiveEffect::SelfBuffOnAllyKo {
                    stat: AtkStat::Atk,
                    amount: 2,
                    max: 3,
                    filter: None,
                }],
                vec![],
            )],
        );
        let ctx = EngineContext::new(1, 42);
        let mut st = blank_state("CAP", "CAP");
        let ko = |st: &mut GameState| {
            apply_on_ko_effects(st, &reg, &ctx, PlayerId::Player1, PlayerId::Player2, "V").unwrap()
        };
        let total = |st: &GameState| -> i32 {
            st.players
                .get(PlayerId::Player1)
                .captain
                .modifiers
                .iter()
                .filter(|m| m.source == "captainSelfKO")
                .map(|m| m.amount)
                .sum()
        };

        ko(&mut st);
        assert_eq!(total(&st), 2);
        ko(&mut st);
        assert_eq!(total(&st), 3, "clamped to max - current == 1");
        assert_eq!(
            st.log.last().unwrap().message,
            "Luffy : +1 ATK permanent (Mugiwara KO)."
        );
        ko(&mut st);
        assert_eq!(total(&st), 3, "no room left: nothing is pushed");
        assert_eq!(st.players.get(PlayerId::Player1).captain.modifiers.len(), 2);
    }

    #[test]
    fn synergy_partner_can_be_the_own_flipped_captain() {
        // Decision §8.1-7 (`RH-004` + `CAP-SHANKS`): a recto captain is off-board,
        // so the bonus only applies once the captain is flipped into a slot.
        let mut rockstar = chr("R", "Rockstar", 2, 1, 4);
        rockstar.synergies = Some(vec![SynergyDef {
            partner_id: "CAP".to_string(),
            atk_bonus: 1,
            on_partner_ko: None,
        }]);
        let reg = CardRegistry::from_sets([vec![rockstar]], [cap("CAP", "Shanks", vec![], vec![])]);
        let mut st = blank_state("CAP", "CAP");
        let r = place(&mut st, &reg, PlayerId::Player1, Slot::V2, "R", 4);

        // Recto: the captain is not on the board.
        recalculate_passive_buffs(&mut st, &reg, PlayerId::Player1).unwrap();
        assert_eq!(atk_bonus(&st, &r), 0);

        // Flipped into a slot: the synergy fires.
        {
            let c = &mut st.players.get_mut(PlayerId::Player1).captain;
            c.flipped = true;
            c.slot = Some(Slot::V1);
        }
        recalculate_passive_buffs(&mut st, &reg, PlayerId::Player1).unwrap();
        assert_eq!(atk_bonus(&st, &r), 1);
        assert!(
            st.cards[&r]
                .modifiers
                .iter()
                .any(|m| m.source == "synergy_CAP")
        );

        // The opponent's Shanks is not *my* partner.
        let mut st2 = blank_state("OTHER", "CAP");
        let r2 = place(&mut st2, &reg, PlayerId::Player1, Slot::V2, "R", 4);
        {
            let c = &mut st2.players.get_mut(PlayerId::Player2).captain;
            c.flipped = true;
            c.slot = Some(Slot::V1);
        }
        let reg2 = CardRegistry::from_sets(
            [vec![{
                let mut d = chr("R", "Rockstar", 2, 1, 4);
                d.synergies = Some(vec![SynergyDef {
                    partner_id: "CAP".to_string(),
                    atk_bonus: 1,
                    on_partner_ko: None,
                }]);
                d
            }]],
            [
                cap("CAP", "Shanks", vec![], vec![]),
                cap("OTHER", "Autre", vec![], vec![]),
            ],
        );
        recalculate_passive_buffs(&mut st2, &reg2, PlayerId::Player1).unwrap();
        assert_eq!(atk_bonus(&st2, &r2), 0);

        // Removed from the board again → the bonus goes away.
        st.players.get_mut(PlayerId::Player1).captain.slot = None;
        recalculate_passive_buffs(&mut st, &reg, PlayerId::Player1).unwrap();
        assert_eq!(atk_bonus(&st, &r), 0);
    }

    #[test]
    fn on_ko_ends_with_a_passive_recalculation() {
        let reg = recalc_registry();
        let mut st = blank_state("CAP", "CAP");
        let l = place(&mut st, &reg, PlayerId::Player1, Slot::V1, "L", 4);
        // A stale captain buff that recalculation must rebuild exactly once.
        st.cards.get_mut(&l).unwrap().modifiers.push(Modifier {
            id: "stale".to_string(),
            stat: ModifierStat::Atk,
            amount: 7,
            source: "captain_CAP".to_string(),
            duration: ModifierDuration::Permanent,
            turns_remaining: None,
        });

        apply_on_ko_effects(
            &mut st,
            &reg,
            &EngineContext::seeded(1),
            PlayerId::Player1,
            PlayerId::Player2,
            "M",
        )
        .unwrap();

        assert_eq!(atk_bonus(&st, &l), 1, "stale +7 replaced by the real +1");
    }
}

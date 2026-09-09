//! Passive effects — port of `src/engine/passives.ts`
//! (`applyStartOfTurnPassives`, `recalculatePassiveBuffs`,
//! `applyEnemyDebuffAuras`, `applyOnKOEffects`, `matchesFilter`).
//!
//! The TS code reads `getCardDef` / `getCaptainDef` from the global registry
//! and `Date.now()` for a few modifier ids; here the [`CardRegistry`] and the
//! [`EngineContext`] (clock) are passed explicitly.

use crate::board::{get_board_characters, get_effective_atk, remove_from_board};
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
                let Some(adj_card) = state.cards.get_mut(&adj_id) else {
                    continue;
                };
                let adj_def = registry.get_card_def(&adj_card.def_id)?;
                let max_pv = adj_def.pv.unwrap_or(adj_card.current_pv);
                adj_card.current_pv = (adj_card.current_pv + amount).min(max_pv);
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
                source: format!("passive_{source_id}"),
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
                let rest = rest.trim_start_matches(char::is_whitespace);
                if rest.starts_with(keyword) {
                    return desc[start..end].parse::<i32>().unwrap_or(0);
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
                            id: format!(
                                "passive_{}_{}_{}",
                                ch.id,
                                buff_stat_str(*stat),
                                target.id
                            ),
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

                for ch in &board_chars {
                    // Check faction filter (e.g. "Mugiwara" or "Marines")
                    let char_def = registry.get_card_def(&ch.def_id)?;
                    let faction_match = (desc.contains("mugiwara") && char_def.has_tag("mugiwara"))
                        || (desc.contains("marine") && char_def.has_tag("marine"))
                        || (!desc.contains("mugiwara") && !desc.contains("marine"));

                    if faction_match {
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
    for ch in &board_chars {
        let def = registry.get_card_def(&ch.def_id)?;
        let Some(synergies) = &def.synergies else {
            continue;
        };
        for syn in synergies {
            // Check if partner is on the board
            let partner_on_board = board_chars.iter().any(|c| c.def_id == syn.partner_id);
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
    if filter.tag.as_ref().is_some_and(|tag| !def.has_tag(tag)) {
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
                if current < *max {
                    cap.modifiers.push(Modifier {
                        id: format!("captainSelfKO_{}", ctx.now()),
                        stat: atk_stat(*stat),
                        amount: *amount,
                        source: "captainSelfKO".to_string(),
                        duration: ModifierDuration::Permanent,
                        turns_remaining: None,
                    });
                    state.add_log(
                        ko_player_id,
                        format!("{cap_def_name} : +{amount} ATK permanent (Mugiwara KO)."),
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
                    source: format!("synergy_rage_{ko_def_id}"),
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
